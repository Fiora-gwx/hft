import csv
import json
from pathlib import Path
import sqlite3
import struct
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace
import report


class Chains(unittest.TestCase):
    def test_rpl_203_network_call_boundaries_and_shared_record_wait(self):
        with tempfile.TemporaryDirectory() as root:
            out = Path(root)
            db = sqlite3.connect(':memory:')
            db.row_factory = sqlite3.Row
            db.execute('CREATE TABLE trace(thread TEXT,' + ','.join('"'+k+'" INTEGER' for k in report.COLUMNS) + ')')
            def point(stage, tsc, **values):
                row = dict.fromkeys(report.COLUMNS, 0)
                row.update(values, stage=stage, tsc=tsc, serial=db.execute('SELECT count(*) FROM trace').fetchone()[0]+1, venue_conn=1)
                db.execute('INSERT INTO trace VALUES ('+','.join('?' for _ in range(16))+')', ['feed']+[row[k] for k in report.COLUMNS])
            point(1,100,v1=1000,v2=300)
            point(35,100,v0=90,v1=44,v2=4096,v3=80)
            for frame, start, done, sw in [(1,110,118,1000),(2,1000,1018,900)]:
                point(4,done+2,frame=frame,v0=100,v2=sw,v3=115)
                point(36,done,frame=frame,v0=start,v1=115,v2=100)
                point(7,done+10,frame=frame,event_hash=frame,kind_inst=0x101)
            point(33,250,v0=0,v1=300,v2=300,v3=245)
            # An older trace has no pre-call timestamp: no zero-duration syscall invented.
            point(33,300,v0=300,v1=300,v2=300)
            report.load_journal(db,[dict(kind_name='ClockStatus',body_tsc=t,realtime_ns=t+1000,calib_err_ns=0,state=1) for t in (0,2000)],out)
            analysis = report.Analysis(db,dict(clock=dict(hz_milli=1_000_000_000_000)))
            analysis.market(out)
            analysis.reads(out)
            analysis.subdivisions()
            def samples(stage):
                return [r[0] for r in db.execute('SELECT ns FROM samples WHERE stage=? ORDER BY rowid',(stage,))]
            self.assertEqual(samples('feed.frame'),[3,18])
            self.assertEqual(samples('feed.next_frame'),[8,18])
            self.assertEqual(samples('feed.decrypt_to_frame'),[5,905])
            self.assertEqual(samples('read.kernel_to_user'),[100])
            self.assertEqual(samples('read.syscall'),[10])
            self.assertEqual(samples('egress.send_syscall'),[5])
            self.assertEqual(samples('receive.frame_wait_sw'),[0,100])
            self.assertEqual(samples('receive.complete_to_read_sw'),[100,100])
            self.assertEqual(samples('receive.read_to_frame'),[18,918])
            self.assertEqual(samples('receive.complete_to_frame_sw'),[118,1018])
            with (out/'event_chains.csv').open() as f:
                chains=list(csv.DictReader(f))
            self.assertEqual([r['last_read_kernel_to_user_ns'] for r in chains],['100','100'])
            self.assertEqual([r['frame_sw_span_ns'] for r in chains],['0','100'])
            with (out/'read_batches.csv').open() as f:
                batches=list(csv.DictReader(f))
            self.assertEqual(len(batches),1)
            self.assertEqual((batches[0]['cipher_offset'],batches[0]['pre_kernel_to_user_ns'],batches[0]['empty_tsc']),('44','90','80'))
            with (out/'receive_frames.csv').open() as f:
                frames=list(csv.DictReader(f))
            self.assertEqual(len(frames),2)
            for f in frames:
                self.assertEqual(int(f['complete_to_frame_sw_ns']),int(f['complete_to_read_sw_ns'])+int(f['read_to_frame_ns']))
                self.assertEqual(f['read_pre_tsc'],'90')

    def test_rpl_203_receive_split_missing_negative_and_frame_weight(self):
        with tempfile.TemporaryDirectory() as root:
            out=Path(root)
            db=sqlite3.connect(':memory:')
            db.row_factory=sqlite3.Row
            db.execute('CREATE TABLE trace(thread TEXT,' + ','.join('"'+k+'" INTEGER' for k in report.COLUMNS) + ')')
            def point(stage, tsc, **values):
                row=dict.fromkeys(report.COLUMNS,0)
                row.update(values,stage=stage,tsc=tsc,serial=db.execute('SELECT count(*) FROM trace').fetchone()[0]+1,venue_conn=513)
                db.execute('INSERT INTO trace VALUES ('+','.join('?' for _ in range(16))+')',['feed']+[row[k] for k in report.COLUMNS])
            point(1,100,v1=1000,v2=80)
            point(4,140,frame=1,v0=100,v2=1010)
            point(36,135,frame=1,v0=120)
            # Several decoded events from one frame must not multiply receive samples.
            point(7,145,frame=1,event_hash=1,kind_inst=0x203)
            point(7,146,frame=1,event_hash=2,kind_inst=0x203)
            # Missing read, and a legacy frame without FrameTiming, stay incomplete.
            point(4,200,frame=2,v0=180,v2=1050)
            point(4,210,frame=3,v0=100,v2=1000)
            report.load_journal(db,[],out)
            analysis=report.Analysis(db,dict(clock=dict(hz_milli=1_000_000_000_000)))
            analysis.market(out)
            tables=analysis.summaries(out)
            wait=next(r for r in tables if r['stage']=='receive.frame_wait_sw')
            self.assertEqual((wait['n'],wait['negative'],wait['p50_ns']),(1,1,0))
            self.assertEqual(db.execute("SELECT count(*) FROM samples WHERE stage='receive.read_to_frame'").fetchone()[0],1)
            self.assertEqual(db.execute("SELECT count(*) FROM samples WHERE stage='receive.complete_to_read_sw'").fetchone()[0],0)
            self.assertEqual(analysis.issues['receive_missing_last_read'],1)
            self.assertEqual(analysis.issues['receive_unsynced_cal'],2)
            with (out/'receive_frames.csv').open() as f:
                frames=list(csv.DictReader(f))
            self.assertEqual(frames[0]['receive_wait_sw_ns'],'-10')
            self.assertEqual(frames[0]['read_pre_tsc'],'')
            self.assertEqual(frames[1]['complete_to_frame_sw_ns'],'')
            self.assertEqual(frames[2]['read_to_frame_ns'],'')

    def test_rpl_203_raw_body_decode_exports_only_timestamp_leaves(self):
        class Batch(dict):
            column_names = ["body", "seq", "tsc", "venue", "conn"]
            num_rows = 1
        batch = Batch(body=[b"binary-prefix-json"], seq=[4], tsc=[123], venue=[10], conn=[2])
        reader = SimpleNamespace(events=lambda *a, **k: [batch])
        module = SimpleNamespace(open_journal=lambda *a, **k: reader,
            decode=SimpleNamespace(raw_rx=lambda b: {"what":1,"payload": b'{"ts":"1792051200000","uid":"PRIVATE","data":[{"T":1792051200001,"signature":"SECRET"}]}'}))
        with tempfile.TemporaryDirectory() as root, patch.dict("sys.modules", {"hft_py": module}):
            root = Path(root)
            db=sqlite3.connect(":memory:")
            db.execute("CREATE TABLE trace(stage INTEGER,v0 INTEGER,v2 INTEGER,v3 INTEGER,venue_conn INTEGER,thread TEXT,frame INTEGER)")
            db.execute("INSERT INTO trace VALUES(34,4,123,1,522,'gw-second',91)")
            report.exchange_times([Path("raw-gw")],root,db=db)
            text = (root / "exchange_times.csv").read_text()
            self.assertNotIn("PRIVATE",text)
            self.assertNotIn("SECRET",text)
            self.assertIn("1792051200001,ms",text)
            self.assertIn("gw-second,91,unique",text)

    def test_rpl_203_md_only_and_each_fill_and_missing_stages(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            trace = root / "trace"
            trace.mkdir()
            out = root / "out"
            out.mkdir()
            schema = dict(version=1, record_bytes=120, columns=report.COLUMNS, threads=["feed", "sequencer", "gw"], clock=dict(hz_milli=1_000_000_000_000))
            (trace / "schema.json").write_text(json.dumps(schema))
            (trace / "completion.json").write_text(json.dumps(dict(closed=True,streams=[dict(thread=n,dropped=0) for n in schema["threads"]])))
            def write(name, points):
                with (trace / (name + ".bin")).open("wb") as stream:
                    for serial, point in enumerate(points, 1):
                        row = dict.fromkeys(report.COLUMNS,0)
                        row.update(point,serial=serial)
                        stream.write(struct.pack("<15Q",*(row[c] for c in report.COLUMNS)))
            write("feed",[
                dict(stage=4,tsc=120,frame=1,venue_conn=1,v0=100,v2=1000,v3=115),
                dict(stage=5,tsc=125,frame=1,venue_conn=1),dict(stage=6,tsc=135,frame=1,venue_conn=1),
                dict(stage=7,tsc=136,frame=1,venue_conn=1,kind_inst=0x101,event_hash=31,v0=900,v1=950),
                dict(stage=8,tsc=140,frame=1,venue_conn=1,kind_inst=0x101,event_hash=31),
                dict(stage=4,tsc=220,frame=6,venue_conn=1,v0=200,v2=2000,v3=215),
                dict(stage=7,tsc=236,frame=6,venue_conn=1,kind_inst=0x101,event_hash=32,v0=1900),
            ])
            write("sequencer",[
                dict(stage=10,tsc=150,event_hash=31),dict(stage=29,tsc=155,event_hash=31),
                dict(stage=11,tsc=160,event_hash=31),dict(stage=12,tsc=165,event_hash=31,input_seq=10),
                dict(stage=13,tsc=166,input_seq=10,v0=1),dict(stage=14,tsc=167,input_seq=10,v0=1),
                dict(stage=15,tsc=168,input_seq=10,v0=1,v1=4),dict(stage=16,tsc=169,input_seq=10,v0=1,v1=4),
                dict(stage=17,tsc=170,input_seq=10,v0=1,v1=4),dict(stage=18,tsc=171,input_seq=10,v0=1,v1=4),
                dict(stage=19,tsc=180,input_seq=10,decision_seq=11),dict(stage=20,tsc=190,input_seq=10,decision_seq=11),
                dict(stage=27,tsc=195,input_seq=10),
            ])
            write("gw",[dict(stage=21,tsc=200,decision_seq=11),dict(stage=25,tsc=235,decision_seq=11),
                dict(stage=32,tsc=240,decision_seq=11,frame=99,venue_conn=1,v0=0,v1=322),
                dict(stage=26,tsc=250,decision_seq=11),
                dict(stage=33,tsc=250,frame=99,venue_conn=1,v0=0,v1=100,v2=322),
                dict(stage=33,tsc=280,frame=99,venue_conn=1,v0=100,v1=222,v2=222)])
            db = sqlite3.connect(":memory:")
            db.row_factory = sqlite3.Row
            loaded = report.load_trace(db,trace,out)
            events = [dict(kind_name='ClockStatus',seq=1,body_tsc=0,realtime_ns=920,phc_ns=920,calib_err_ns=5,state=1),
                      dict(kind_name='ClockStatus',seq=20,body_tsc=1000,realtime_ns=1920,phc_ns=1920,calib_err_ns=5,state=1),
                      dict(kind_name="OrderDecision",seq=11,trigger_seq=10,slot=4,gen=2,venue=1,account=1,action=1,inst=4,intent_no=1,strategy=1),
                      dict(kind_name="OrderEvent",seq=12,slot=4,gen=2,venue=1,account=1,epoch=7,what=1,rx_tsc=300),
                      dict(kind_name="OrderEvent",seq=13,slot=4,gen=2,venue=1,account=1,epoch=7,what=3,rx_tsc=350,trade_id=101),
                      dict(kind_name="OrderEvent",seq=14,slot=4,gen=2,venue=1,account=1,epoch=7,what=4,rx_tsc=400,trade_id=102),
                      # Independent order tables reuse the same slot/gen in live two-leg runs.
                      dict(kind_name="OrderEvent",seq=15,slot=4,gen=2,venue=10,account=1,epoch=7,what=4,rx_tsc=900,trade_id=201),
                      dict(kind_name="OrderEvent",seq=16,slot=4,gen=2,venue=1,account=2,epoch=7,what=4,rx_tsc=950,trade_id=202),
                      dict(kind_name="OrderEvent",seq=17,slot=4,gen=2,venue=1,what=4,rx_tsc=999,trade_id=203)]
            report.load_journal(db,events,out)
            analysis = report.Analysis(db,loaded)
            analysis.market(out)
            analysis.orders(out)
            analysis.subdivisions()
            tables = analysis.summaries(out)
            with (out / "event_chains.csv").open() as stream:
                rows = list(csv.DictReader(stream))
            self.assertEqual(len(rows),2)
            self.assertEqual(rows[0]["input_seq"],"10")
            self.assertEqual(rows[1]["input_seq"],"")
            with (out / "order_chains.csv").open() as stream:
                orders = list(csv.DictReader(stream))
            self.assertEqual([r["report_what"] for r in orders],["1","3","4"])
            self.assertEqual([r["report_seq"] for r in orders],["12","13","14"])
            self.assertTrue(all(r["account"] == "1" and r["report_epoch"] == "7" for r in orders))
            self.assertEqual(next(t["n"] for t in tables if t["stage"]=="ack_fill_rtt.4"),1)
            self.assertEqual(next(t["p99_ns"] for t in tables if t["stage"]=="ack_fill_rtt.4"),150)
            self.assertTrue(all(r["send_complete_tsc"] == "280" and r["tx_write_count"] == "2" for r in orders))
            self.assertTrue(all(r["oms_start_tsc"]=="170" and r["risk_start_tsc"]=="168" and r["strategy_start_tsc"]=="166" and r["input_rx_tsc"]=="100" for r in orders))
            self.assertEqual(next(t["p50_ns"] for t in tables if t["stage"]=="tick_to_trade.userspace"),150)
            self.assertEqual(next(t["p50_ns"] for t in tables if t["stage"]=="tick_to_trade.sw"),170)
            self.assertEqual(rows[0]['rx_realtime_ns'], '1020')
            self.assertEqual(rows[0]['kernel_to_user_ns'], '20')
            self.assertEqual(next(t["p99_ns"] for t in tables if t["stage"]=="feed.decode.bbo"),10)
            self.assertGreater(analysis.issues["missing_egress.encrypt"],0)
            self.assertFalse(any(t['stage']=='feed.frame' for t in tables))

    def test_rpl_203_sw_calibration_brackets_steps_and_error_filter(self):
        rows=[dict(body_tsc=t,realtime_ns=r,phc_ns=r,calib_err_ns=5,state=1)
              for t,r in [(0,1000),(100,1100),(200,1220)]]
        calibration=report.Calibration(rows)
        self.assertEqual(calibration.realtime(150),(1160,17))
        self.assertIsNone(calibration.realtime(-1))
        self.assertIsNone(calibration.realtime(200))
        self.assertIsNone(report.Calibration(rows,16).realtime(150))
        rows[2]['state']=3
        self.assertIsNone(report.Calibration(rows).realtime(150))
        repeated = report.Calibration(rows + [dict(rows[0], seq=100, tsc=999, flags=0)])
        self.assertEqual(repeated.duplicate_anchors,1)
        self.assertEqual(len(repeated.rows),3)
        with self.assertRaisesRegex(ValueError, 'conflicting calibration anchor'):
            report.Calibration(rows + [dict(rows[0], realtime_ns=1001)])

    def test_rpl_203_sw_rate_error_without_phc(self):
        rows=[dict(body_tsc=t,realtime_ns=r,phc_ns=0,calib_err_ns=5,state=1)
              for t,r in [(0,1000),(100,1100),(200,1220)]]
        self.assertEqual(report.Calibration(rows).realtime(150),(1160,17))
        self.assertIsNone(report.Calibration(rows,16).realtime(150))

    def test_rpl_203_negative_and_ambiguous_are_not_zero_samples(self):
        db = sqlite3.connect(":memory:")
        db.row_factory = sqlite3.Row
        db.execute("CREATE TABLE trace(stage INTEGER,event_hash TEXT,tsc INTEGER)")
        db.executemany("INSERT INTO trace VALUES(8,'same',?)",[(10,),(11,)])
        analysis = report.Analysis(db,{"clock":{"hz_milli":1_000_000_000_000}})
        self.assertIsNone(analysis.unique(8,event_hash="same"))
        analysis.interval(1,"queue","chain",110,100)
        analysis.interval(1,"queue","missing",None,100)
        self.assertEqual(db.execute("SELECT ns FROM samples").fetchall()[0][0],-10)
        self.assertEqual(analysis.issues["negative_queue"],1)
        self.assertEqual(analysis.issues["missing_queue"],1)


if __name__ == "__main__":
    unittest.main()
