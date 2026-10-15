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
            events = [dict(kind_name="OrderDecision",seq=11,trigger_seq=10,slot=4,gen=2,venue=1,action=1,inst=4,intent_no=1,strategy=1),
                      dict(kind_name="OrderEvent",seq=12,slot=4,gen=2,what=1,rx_tsc=300),
                      dict(kind_name="OrderEvent",seq=13,slot=4,gen=2,what=3,rx_tsc=350,trade_id=101),
                      dict(kind_name="OrderEvent",seq=14,slot=4,gen=2,what=4,rx_tsc=400,trade_id=102)]
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
            self.assertTrue(all(r["send_complete_tsc"] == "280" and r["tx_write_count"] == "2" for r in orders))
            self.assertTrue(all(r["oms_start_tsc"]=="170" and r["risk_start_tsc"]=="168" and r["strategy_start_tsc"]=="166" and r["input_rx_tsc"]=="100" for r in orders))
            self.assertEqual(next(t["p50_ns"] for t in tables if t["stage"]=="tick_to_trade"),150)
            self.assertEqual(next(t["p99_ns"] for t in tables if t["stage"]=="feed.decode.bbo"),10)
            self.assertGreater(analysis.issues["missing_egress.encrypt"],0)

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
