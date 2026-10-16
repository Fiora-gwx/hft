"""Read-only latency-chain extraction. Uses the released hft_py journal reader."""
import argparse
import bisect
import collections
import csv
import json
import math
from pathlib import Path
import sqlite3
import struct

COLUMNS = "tsc serial dropped stage input_seq decision_seq order frame event_hash venue_conn kind_inst v0 v1 v2 v3".split()
STAGES = ("read decrypt_start decrypt_end frame decode_start decode_end decoded_event feed_push rep_push input_pop process_start journal_input strategy_start strategy_end risk_start risk_end oms_start oms_end decision out_push out_pop encode_start encode_end encrypt_start encrypt_end send process_end tx_stamp input_handed exec_start exec_end").split()
STAGES += ["tx_queued", "tx_write", "raw_record"]
SAFE = set("seq tsc proc_ns flags kind venue conn inst account strategy trigger_seq decision_seq slot gen target_slot action link_role reason intent_no reject what detail trade_id exch_order_id ex_ts_ns ex_send_ns hw_rx_ns sw_rx_ns rx_tsc decided_tsc sent_tsc group status tsc_hz_milli realtime_ns phc_ns chrony_offset_ns max_error_ns rms_ns calib_err_ns source state".split())
KINDS = ["Bbo", "Trade", "Funding", "MarkPrice", "IndexPrice", "Liquidation", "L2Snapshot", "OrderDecision", "SendReport", "OrderEvent", "ExecDecision", "ClockStatus", "LatencySample"]
SAFE.update(("body_tsc", "epoch"))
CHAIN_COLUMNS = "chain thread frame event_hash venue kind input_seq ex_ts_ns ex_send_ns hw_rx_ns sw_rx_ns rx_tsc decrypt_done_tsc frame_done_tsc decode_start_tsc decode_done_tsc ring_push_tsc ring_pop_tsc handed_tsc process_start_tsc journal_input_tsc process_end_tsc".split()
CHAIN_COLUMNS += "rx_realtime_ns calibration_error_ns kernel_to_user_ns".split()


class Calibration:
    """SW-domain counterpart of hft-telemetry ClockCal; never extrapolate tails."""
    def __init__(self, rows, max_error=1000):
        fields = "body_tsc realtime_ns phc_ns tsc_hz_milli chrony_offset_ns max_error_ns rms_ns calib_err_ns source state".split()
        self.rows = []
        self.duplicate_anchors = 0
        for row in sorted(rows, key=lambda r: r['body_tsc']):
            if self.rows and row['body_tsc'] == self.rows[-1]['body_tsc']:
                if any(row.get(k) != self.rows[-1].get(k) for k in fields):
                    raise ValueError('conflicting calibration anchor')
                # Segment boot blocks repeat the current ClockStatus with new record headers.
                self.duplicate_anchors += 1
            else:
                self.rows.append(row)
        self.ticks = [r['body_tsc'] for r in self.rows]
        self.max_error = max_error

    def realtime(self, tick):
        index = bisect.bisect_right(self.ticks, tick) - 1
        if index < 0 or index + 1 >= len(self.rows):
            return None
        a, b = self.rows[index:index+2]
        if a.get('state') == 3 or b.get('state') == 3:
            return None
        length = b['body_tsc'] - a['body_tsc']
        error = max(a['calib_err_ns'], b['calib_err_ns'])
        if index:
            previous = self.rows[index-1]
            dt = a['body_tsc'] - previous['body_tsc']
            dp = b['realtime_ns'] - a['realtime_ns']
            prior_dp = a['realtime_ns'] - previous['realtime_ns']
            numerator = abs(dp * dt - prior_dp * length) * abs(dp)
            denominator = 2 * length * dt
            error += (2 * numerator + denominator) // (2 * denominator)
        if error > self.max_error:
            return None
        numerator = (tick - a['body_tsc']) * (b['realtime_ns'] - a['realtime_ns'])
        return a['realtime_ns'] + (2 * numerator + length) // (2 * length), error


def scalar(value):
    return value.as_py() if hasattr(value, "as_py") else value


def csv_file(path, columns):
    stream = path.open("w", newline="", encoding="utf-8")
    writer = csv.DictWriter(stream, fieldnames=columns, extrasaction="ignore")
    writer.writeheader()
    return stream, writer


def signed(value):
    return value - (1 << 64) if value >= 1 << 63 else value


def load_trace(db, trace, out):
    schema = json.loads((trace / "schema.json").read_text())
    if schema["version"] != 1 or schema["record_bytes"] != 120 or schema["columns"] != COLUMNS:
        raise ValueError("unsupported trace schema")
    definitions = ",".join('"' + c + '" ' + ("TEXT" if c == "event_hash" else "INTEGER") for c in COLUMNS)
    db.execute("CREATE TABLE trace(thread TEXT," + definitions + ")")
    output, writer = csv_file(out / "raw_points.csv", ["thread"] + COLUMNS + ["stage_name"])
    coverage = {}
    try:
        for name in schema["threads"]:
            path = trace / (name.replace(":", "_") + ".bin")
            if path.stat().st_size % 120:
                raise ValueError("truncated trace record: " + path.name)
            serial = 0
            count = 0
            dropped = 0
            with path.open("rb") as stream:
                while block := stream.read(120 * 4096):
                    rows = []
                    for words in struct.iter_unpack("<15Q", block):
                        row = dict(zip(COLUMNS, words))
                        if row["serial"] <= serial or not 1 <= row["stage"] <= len(STAGES):
                            raise ValueError("invalid trace ordering/stage")
                        serial = row["serial"]
                        dropped = max(dropped, row["dropped"])
                        row["event_hash"] = f'{row["event_hash"]:016x}'
                        writer.writerow(dict(row, thread=name, stage_name=STAGES[row["stage"] - 1]))
                        rows.append([name] + [row[c] if c == "event_hash" else signed(row[c]) for c in COLUMNS])
                        count += 1
                    db.executemany("INSERT INTO trace VALUES(" + ",".join("?" for _ in range(16)) + ")", rows)
            coverage[name] = {"records": count, "reported_dropped": dropped, "last_serial": serial}
    finally:
        output.close()
    for index, columns in enumerate(["stage,event_hash", "stage,input_seq", "stage,decision_seq", "thread,frame,stage", "thread,tsc,stage", "thread,serial", "stage,v0,v2,v3,venue_conn"]):
        db.execute(f"CREATE INDEX trace_{index} ON trace({columns})")
    completion = trace / "completion.json"
    closed = completion.exists() and json.loads(completion.read_text()).get("closed", False)
    if closed:
        for row in json.loads(completion.read_text())["streams"]:
            coverage[row["thread"]]["final_dropped"] = row["dropped"]
    schema["coverage"] = coverage
    schema["closed"] = closed
    return schema


def journal_rows(path, hot=None):
    import hft_py
    journal = hft_py.open_journal(str(path), hot_root=hot)
    for kind in KINDS:
        for batch in journal.events(kind, batch_rows=2048):
            names = [n for n in batch.column_names if n in SAFE or n.startswith("h.") and n[2:] in SAFE]
            for i in range(batch.num_rows):
                row = {n.removeprefix("h."): scalar(batch[n][i]) for n in names}
                row["kind_name"] = kind
                yield row


def load_journal(db, rows, out):
    db.execute("CREATE TABLE events(seq INTEGER,kind TEXT,slot INTEGER,gen INTEGER,body TEXT,trigger_seq INTEGER,group_id INTEGER,venue INTEGER,account INTEGER)")
    output, writer = csv_file(out / "journal_events.csv", ["kind_name"] + sorted(SAFE))
    try:
        for row in rows:
            writer.writerow(row)
            db.execute("INSERT INTO events VALUES(?,?,?,?,?,?,?,?,?)", (row.get("seq", 0), row["kind_name"], row.get("slot", 0), row.get("gen", 0), json.dumps(row),row.get("trigger_seq",0),row.get("group",0),row.get("venue"),row.get("account")))
    finally:
        output.close()
    db.execute("CREATE INDEX event_seq ON events(seq)")
    db.execute("CREATE INDEX event_order ON events(kind,venue,account,slot,gen)")
    db.execute("CREATE INDEX event_trigger ON events(kind,trigger_seq,group_id)")


def exchange_times(raw_paths, out, hot=None, db=None):
    """Export timestamp leaves only. Never persist private-stream JSON or identifiers."""
    import hft_py
    stream, writer = csv_file(out / "exchange_times.csv", "journal raw_seq venue conn rx_tsc trace_thread trace_frame trace_match json_path field value unit".split())
    fields = {"E", "T", "ts", "cTime", "uTime", "fillTime", "tradeTime", "requestTime", "transactTime", "updateTime", "workingTime"}
    def walk(value, path=""):
        if isinstance(value, dict):
            for key, item in value.items():
                child = path + "." + (key if key in fields or key in {"data","result","o","orders","balances","positions","arg"} else "*")
                if key in fields and isinstance(item,(str,int)) and str(item).isdigit():
                    yield child,key,int(item)
                elif isinstance(item,(dict,list)):
                    yield from walk(item,child)
        elif isinstance(value,list):
            for i,item in enumerate(value):
                yield from walk(item,path + "[" + str(i) + "]")
    try:
        for path in raw_paths:
            journal = hft_py.open_journal(str(path), hot_root=hot)
            for batch in journal.events("RawRx",batch_rows=1024):
                if "body" not in batch.column_names:
                    raise ValueError("RawRx reader did not expose its binary body")
                for i in range(batch.num_rows):
                    decoded = hft_py.decode.raw_rx(scalar(batch["body"][i]))
                    try:
                        body = json.loads(decoded["payload"])
                    except (ValueError,UnicodeDecodeError,TypeError):
                        continue
                    base = {"journal":path.name,"raw_seq":scalar(batch["seq"][i]),"rx_tsc":scalar(batch["tsc"][i]),"venue":scalar(batch["venue"][i]),"conn":scalar(batch["conn"][i])}
                    if db is not None:
                        candidates = db.execute("SELECT thread,frame FROM trace WHERE stage=34 AND v0=? AND v2=? AND v3=? AND venue_conn=? AND frame>0 LIMIT 2",
                            (base["raw_seq"],base["rx_tsc"],decoded["what"],base["venue"] | base["conn"] << 8)).fetchall()
                        base["trace_match"] = "unique" if len(candidates)==1 else "ambiguous" if candidates else "missing"
                        if len(candidates)==1:
                            base["trace_thread"],base["trace_frame"]=candidates[0]
                    for json_path,field,value in walk(body):
                        # Protocol fields are retained verbatim; lengths only label units,
                        # never silently scale a value whose precision is uncertain.
                        unit = {10:"s",13:"ms",16:"us",19:"ns"}.get(len(str(value)),"unspecified")
                        writer.writerow(dict(base,json_path=json_path,field=field,value=value,unit=unit))
    finally:
        stream.close()


class Analysis:
    def __init__(self, db, schema, cal_max_err_ns=1000):
        self.db = db
        self.schema = schema
        self.hz = schema["clock"]["hz_milli"]
        if self.hz <= 0:
            raise ValueError("invalid counter frequency")
        self.db.execute("CREATE TABLE samples(venue INTEGER,stage TEXT,chain TEXT,ns INTEGER)")
        self.issues = collections.Counter()
        clocks = []
        if db.execute("SELECT 1 FROM sqlite_master WHERE name='events'").fetchone():
            clocks = [json.loads(row[0]) for row in db.execute("SELECT body FROM events WHERE kind='ClockStatus'")]
        self.calibration = Calibration(clocks, cal_max_err_ns)
        if self.calibration.duplicate_anchors:
            self.issues['repeated_calibration_anchors'] = self.calibration.duplicate_anchors

    def sample(self, venue, name, chain, delta):
        self.db.execute("INSERT INTO samples VALUES(?,?,?,?)", (venue, name, chain, delta))
        if delta < 0:
            self.issues["negative_" + name] += 1

    def unique(self, stage, **filters):
        query = "SELECT * FROM trace WHERE stage=?" + "".join(' AND "' + k + '"=?' for k in filters)
        rows = self.db.execute(query + " LIMIT 2", [stage, *filters.values()]).fetchall()
        if len(rows) > 1:
            self.issues["ambiguous_" + STAGES[stage - 1]] += 1
            return None
        return dict(rows[0]) if rows else None

    def interval(self, venue, name, chain, start, end):
        if not start or not end:
            self.issues["missing_" + name] += 1
            return
        delta = (end - start) * 1_000_000_000_000 // self.hz
        self.sample(venue, name, chain, delta)

    def market(self, out):
        cols = CHAIN_COLUMNS
        output, writer = csv_file(out / "event_chains.csv", cols)
        self.db.execute("CREATE TABLE chains(input_seq INTEGER,chain TEXT,venue INTEGER,rx_tsc INTEGER,kind INTEGER,body TEXT)")
        try:
            for raw in self.db.execute("SELECT * FROM trace WHERE stage=7 ORDER BY thread,serial"):
                d = dict(raw)
                chain = f'{d["thread"]}:{d["frame"]}:{d["serial"]}'
                row = dict(chain=chain, thread=d["thread"], frame=d["frame"], event_hash=d["event_hash"], venue=d["venue_conn"] & 255, kind=d["kind_inst"] & 65535)
                frame = self.unique(4, thread=d["thread"], frame=d["frame"])
                if frame:
                    row.update(rx_tsc=frame["v0"], hw_rx_ns=frame["v1"], sw_rx_ns=frame["v2"], decrypt_done_tsc=frame["v3"], frame_done_tsc=frame["tsc"])
                    if row['sw_rx_ns']:
                        calibrated = self.calibration.realtime(row['rx_tsc'])
                        if calibrated:
                            realtime, error = calibrated
                            row.update(rx_realtime_ns=realtime, calibration_error_ns=error,
                                kernel_to_user_ns=realtime-row['sw_rx_ns'])
                            self.sample(row['venue'], 'kernel_to_user', chain, row['kernel_to_user_ns'])
                        else:
                            self.issues['unsynced_cal'] += 1
                if 0x100 <= row["kind"] < 0x200:
                    row.update(ex_ts_ns=d["v0"], ex_send_ns=d["v1"])
                elif row["kind"] == 0x203:
                    row["ex_ts_ns"] = d["v0"]
                for stage, column in [(5,"decode_start_tsc"),(6,"decode_done_tsc")]:
                    point = self.unique(stage, thread=d["thread"], frame=d["frame"])
                    if point:
                        row[column] = point["tsc"]
                for stage, column in [(8 if row["kind"] < 0x200 else 9,"ring_push_tsc"),(10,"ring_pop_tsc"),(29,"handed_tsc"),(11,"process_start_tsc"),(12,"journal_input_tsc")]:
                    point = self.unique(stage, event_hash=d["event_hash"])
                    if point:
                        row[column] = point["tsc"]
                        if stage == 12:
                            row["input_seq"] = point["input_seq"]
                if row.get("input_seq"):
                    point = self.unique(27, input_seq=row["input_seq"])
                    if point:
                        row["process_end_tsc"] = point["tsc"]
                    self.db.execute("INSERT INTO chains VALUES(?,?,?,?,?,?)", (row["input_seq"], chain, row["venue"], row.get("rx_tsc"), row["kind"],json.dumps(row)))
                else:
                    self.issues["decoded_without_seq_input"] += 1
                writer.writerow(row)
                for name, start, end in [("feed.read_to_decrypt","rx_tsc","decrypt_done_tsc"),("feed.frame","decrypt_done_tsc","frame_done_tsc"),("feed.decode","decode_start_tsc","decode_done_tsc"),("feed.publish","decode_done_tsc","ring_push_tsc"),("queue","ring_push_tsc","ring_pop_tsc"),("seq.intake","ring_pop_tsc","process_start_tsc"),("seq.journal_input","process_start_tsc","journal_input_tsc"),("seq.process","process_start_tsc","process_end_tsc")]:
                    suffix = ".bbo" if name == "feed.decode" and row["kind"] == 0x101 else ""
                    self.interval(row["venue"], name + suffix, chain, row.get(start), row.get(end))
        finally:
            output.close()
        self.db.execute("CREATE INDEX chain_input ON chains(input_seq)")

    def orders(self, out):
        cols = "decision_seq trigger_seq venue account slot gen group link_role reason action intent_no reject decision_tsc out_push_tsc out_pop_tsc encode_start_tsc encode_done_tsc encrypt_start_tsc encrypt_done_tsc send_return_tsc send_complete_tsc tx_write_count report_seq report_what report_epoch ack_fill_rx_tsc report_ex_ts_ns exch_order_id trade_id".split()
        cols += "strategy risk_owner_intent_no strategy_start_tsc strategy_end_tsc risk_start_tsc risk_end_tsc oms_start_tsc oms_end_tsc".split()
        cols += ["input_"+c for c in CHAIN_COLUMNS]
        output, writer = csv_file(out / "order_chains.csv", cols)
        try:
            for (body,) in self.db.execute("SELECT body FROM events WHERE kind='OrderDecision' ORDER BY seq"):
                event = json.loads(body)
                row = {k: v for k, v in event.items() if k in cols}
                row["decision_seq"] = event["seq"]
                trigger_seq=event.get("trigger_seq",0)
                inputs=self.db.execute("SELECT body FROM chains WHERE input_seq=? LIMIT 2",(trigger_seq,)).fetchall()
                if len(inputs)==1:
                    row.update({"input_"+k:v for k,v in json.loads(inputs[0][0]).items()})
                for stage,column in [(13,"strategy_start_tsc"),(14,"strategy_end_tsc")]:
                    point=self.unique(stage,input_seq=trigger_seq,v0=event.get("strategy",0))
                    if point: row[column]=point["tsc"]
                for stage,column in [(17,"oms_start_tsc"),(18,"oms_end_tsc")]:
                    point=self.unique(stage,input_seq=trigger_seq,v0=event.get("intent_no",0),v1=event.get("inst",0))
                    if point: row[column]=point["tsc"]
                risk_owner=event
                if event.get("link_role")==2:
                    peers=[json.loads(b[0]) for b in self.db.execute("SELECT body FROM events WHERE kind='OrderDecision' AND trigger_seq=? AND group_id=?",(trigger_seq,event.get("group",0)))]
                    peers=[r for r in peers if r.get("trigger_seq")==trigger_seq and r.get("group")==event.get("group") and r.get("link_role")==1 and r.get("action")==1]
                    if len(peers)==1: risk_owner=peers[0]
                row["risk_owner_intent_no"]=risk_owner.get("intent_no",0)
                for stage,column in [(15,"risk_start_tsc"),(16,"risk_end_tsc")]:
                    point=self.unique(stage,input_seq=trigger_seq,v0=risk_owner.get("intent_no",0),v1=risk_owner.get("inst",0))
                    if point: row[column]=point["tsc"]
                for stage, column in [(19,"decision_tsc"),(20,"out_push_tsc"),(21,"out_pop_tsc"),(22,"encode_start_tsc"),(23,"encode_done_tsc"),(24,"encrypt_start_tsc"),(25,"encrypt_done_tsc"),(26,"send_return_tsc")]:
                    point = self.unique(stage, decision_seq=event["seq"])
                    if point:
                        row[column] = point["tsc"]
                venue = event.get("venue", 0)
                chain = "order:" + str(event["seq"])
                queued = self.unique(32, decision_seq=event["seq"])
                if queued:
                    writes = self.db.execute("SELECT tsc,v0,v1 FROM trace WHERE stage=33 AND thread=? AND frame=? AND venue_conn=? AND tsc>=? ORDER BY serial",
                        (queued["thread"],queued["frame"],queued["venue_conn"],queued["tsc"]))
                    count = 0
                    for stamp, offset, result in writes:
                        if offset >= queued["v1"]:
                            break
                        if result > 0 and offset + result > queued["v0"]:
                            count += 1
                            if offset + result >= queued["v1"]:
                                row["send_complete_tsc"] = stamp
                                break
                    row["tx_write_count"] = count
                    self.interval(venue,"egress.write_complete",chain,row.get("encrypt_done_tsc"),row.get("send_complete_tsc"))
                for name, start, end in [("egress.queue","out_push_tsc","out_pop_tsc"),("egress.encode","encode_start_tsc","encode_done_tsc"),("egress.encrypt","encrypt_start_tsc","encrypt_done_tsc"),("egress.send","encrypt_done_tsc","send_return_tsc"),("egress","decision_tsc","send_return_tsc")]:
                    self.interval(venue, name, chain, row.get(start), row.get(end))
                trigger = self.db.execute("SELECT rx_tsc,kind FROM chains WHERE input_seq=?", (event.get("trigger_seq", 0),)).fetchall()
                if len(trigger) == 1:
                    name = "tick_to_trade" if 0x100 <= trigger[0][1] < 0x200 else "rep_to_order"
                    self.interval(venue, name + '.userspace', chain, trigger[0][0], row.get("send_return_tsc"))
                    kernel = row.get('input_kernel_to_user_ns')
                    if kernel is not None and kernel >= 0 and trigger[0][0] and row.get('send_return_tsc'):
                        delta = (row['send_return_tsc']-trigger[0][0])*1_000_000_000_000//self.hz
                        self.sample(venue, name+'.sw', chain, kernel+delta)
                reports = self.db.execute("SELECT body FROM events WHERE kind='OrderEvent' AND venue=? AND account=? AND slot=? AND gen=? ORDER BY seq", (event.get("venue"), event.get("account"), event.get("slot", 0), event.get("gen", 0))).fetchall()
                matched = 0
                for (report_body,) in reports:
                    report = json.loads(report_body)
                    allowed = {1:{1,2,3,4,9,11,12}, 2:{5,6,11}, 3:{7,8,11}, 5:{10,11}}
                    if report.get("what") not in allowed.get(event.get("action"),set()) or report.get("seq",0) <= event["seq"]:
                        continue
                    writer.writerow(dict(row, report_seq=report.get("seq"), report_what=report.get("what"), report_epoch=report.get("epoch"), ack_fill_rx_tsc=report.get("rx_tsc"), report_ex_ts_ns=report.get("ex_ts_ns"), exch_order_id=report.get("exch_order_id"), trade_id=report.get("trade_id")))
                    matched += 1
                    self.interval(venue, "ack_fill_rtt." + str(report.get("what")), chain, row.get("send_return_tsc"), report.get("rx_tsc"))
                if not matched:
                    writer.writerow(row)
        finally:
            output.close()

    def subdivisions(self):
        for start, end, name in [(2,3,"feed.tls_decrypt"),(13,14,"seq.strategy"),(15,16,"seq.risk"),(17,18,"seq.oms"),(30,31,"seq.exec")]:
            for row in self.db.execute("SELECT * FROM trace WHERE stage=? ORDER BY thread,serial", (start,)):
                a = dict(row)
                b = self.db.execute("SELECT * FROM trace WHERE thread=? AND stage IN (?,?) AND serial>? ORDER BY serial LIMIT 1", (a["thread"],start,end,a["serial"])).fetchone()
                if b and b["stage"] == end and b["input_seq"] == a["input_seq"] and b["v0"] == a["v0"]:
                    self.interval(a["venue_conn"] & 255, name, f'{a["thread"]}:{a["serial"]}', a["tsc"], b["tsc"])
                else:
                    self.issues["unpaired_" + name] += 1

    def pair_skew(self, out):
        stream, writer = csv_file(out / "leg_skew.csv", "trigger_seq group first_decision second_decision first_venue second_venue push_skew_ns send_skew_ns".split())
        pairs = collections.defaultdict(list)
        for (body,) in self.db.execute("SELECT body FROM events WHERE kind='OrderDecision'"):
            row = json.loads(body)
            if row.get("action") == 1 and row.get("link_role") in (1,2) and row.get("reject",0) == 0:
                pairs[(row.get("trigger_seq"),row.get("group"))].append(row)
        try:
            for (trigger,group), rows in pairs.items():
                first = [r for r in rows if r["link_role"] == 1]
                second = [r for r in rows if r["link_role"] == 2]
                if len(first) != 1 or len(second) != 1:
                    self.issues["ambiguous_or_non_simultaneous_pair"] += 1
                    continue
                a,b = first[0],second[0]
                row = dict(trigger_seq=trigger,group=group,first_decision=a["seq"],second_decision=b["seq"],first_venue=a.get("venue"),second_venue=b.get("venue"))
                for stage,label in [(20,"push_skew_ns"),(26,"send_skew_ns")]:
                    pa,pb = self.unique(stage,decision_seq=a["seq"]),self.unique(stage,decision_seq=b["seq"])
                    if pa and pb:
                        row[label] = (pb["tsc"]-pa["tsc"])*1_000_000_000_000//self.hz
                writer.writerow(row)
        finally:
            stream.close()

    def summaries(self, out):
        stream, writer = csv_file(out / "stages.csv", "venue stage n negative p50_ns p90_ns p99_ns p999_ns max_ns".split())
        hist, hist_writer = csv_file(out / "histograms.csv", ["venue","stage","bucket_lower_ns","count"])
        summaries = []
        try:
            groups = self.db.execute("SELECT venue,stage,count(*),sum(ns<0) FROM samples GROUP BY venue,stage").fetchall()
            for venue, stage, total, negative in groups:
                count = total - negative
                row = dict(venue=venue, stage=stage, n=count, negative=negative)
                for label, fraction in [("p50_ns",.5),("p90_ns",.9),("p99_ns",.99),("p999_ns",.999),("max_ns",1)]:
                    offset = max(0, math.ceil(count * fraction) - 1)
                    result = self.db.execute("SELECT ns FROM samples WHERE venue=? AND stage=? AND ns>=0 ORDER BY ns LIMIT 1 OFFSET ?", (venue,stage,offset)).fetchone()
                    row[label] = result[0] if result else None
                writer.writerow(row)
                summaries.append(row)
                buckets = collections.Counter()
                for (ns,) in self.db.execute("SELECT ns FROM samples WHERE venue=? AND stage=? AND ns>=0", (venue,stage)):
                    buckets[0 if ns == 0 else 1 << (ns.bit_length()-1)] += 1
                for lower, n in sorted(buckets.items()):
                    hist_writer.writerow(dict(venue=venue,stage=stage,bucket_lower_ns=lower,count=n))
        finally:
            stream.close()
            hist.close()
        return summaries


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--trace", type=Path, required=True)
    parser.add_argument("--journal", type=Path, required=True)
    parser.add_argument("--hot")
    parser.add_argument("--raw", type=Path, action="append", default=[])
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--cal-max-err-ns", type=int, default=1000)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    db = sqlite3.connect(args.out / "analysis.sqlite")
    db.row_factory = sqlite3.Row
    db.execute("PRAGMA cache_size=-32768")
    schema = load_trace(db, args.trace, args.out)
    schema['analysis_cal_max_err_ns'] = args.cal_max_err_ns
    load_journal(db, journal_rows(args.journal,args.hot), args.out)
    if args.raw:
        exchange_times(args.raw,args.out,args.hot,db)
    if args.cal_max_err_ns <= 0:
        raise ValueError('calibration error threshold must be positive')
    analysis = Analysis(db, schema, args.cal_max_err_ns)
    analysis.market(args.out)
    analysis.orders(args.out)
    analysis.subdivisions()
    analysis.pair_skew(args.out)
    tables = analysis.summaries(args.out)
    db.commit()
    db.close()
    summary = {"schema": schema,"issues": dict(analysis.issues),"stages":tables,"percentiles":"empirical nearest rank; small-n tails are descriptive, not acceptance evidence", "kernel_tx":"not implemented by current Connector; absent", "exchange_clock":"exchange timestamps preserved; no exchange-to-host synchronization assumption"}
    (args.out / "summary.json").write_text(json.dumps(summary,indent=2)+"\n",encoding="utf-8")
    print(json.dumps({"output":str(args.out),"closed":schema["closed"],"issues":dict(analysis.issues)}))


if __name__ == "__main__":
    main()
