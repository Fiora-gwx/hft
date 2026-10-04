"""Light, read-only network probe: DNS, TCP connect RTT, TLS handshake time per exchange endpoint.

Run on the Tokyo server. No exchange API calls, no auth; ~15 TCP connects + 3 TLS handshakes per host,
spaced 200 ms apart, so the load on the production box and on the exchanges is negligible.
"""
import json, socket, ssl, statistics, sys, time

HOSTS = {
    "binance": ["fstream.binance.com", "ws-fapi.binance.com", "fapi.binance.com", "stream.binance.com",
                "ws-api.binance.com", "api.binance.com", "api1.binance.com", "api2.binance.com",
                "api3.binance.com", "api4.binance.com"],
    "gate": ["api.gateio.ws", "fx-ws.gateio.ws", "ws.gate.io", "fx-api.gateio.ws"],
    "okx": ["ws.okx.com", "www.okx.com"],
    "bybit": ["stream.bybit.com", "api.bybit.com"],
    "bitget": ["ws.bitget.com", "api.bitget.com"],
    "kucoin": ["ws-api-spot.kucoin.com", "api.kucoin.com", "api-futures.kucoin.com"],
    "hyperliquid": ["api.hyperliquid.xyz"],
    "bsc": ["bsc-dataseed.bnbchain.org"],
}
N_TCP, N_TLS = 15, 3


def tcp_rtt(ip, port=443):
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(3)
    t = time.perf_counter()
    try:
        s.connect((ip, port))
        return (time.perf_counter() - t) * 1e3
    except OSError:
        return None
    finally:
        s.close()


def tls_ms(host, ip):
    ctx = ssl.create_default_context()
    raw = socket.create_connection((ip, 443), timeout=5)
    t0 = time.perf_counter()
    try:
        s = ctx.wrap_socket(raw, server_hostname=host)
        dt = (time.perf_counter() - t0) * 1e3
        ver = s.version()
        s.close()
        return dt, ver
    except Exception as e:  # report, keep going
        raw.close()
        return None, type(e).__name__


out = []
for venue, hosts in HOSTS.items():
    for h in hosts:
        try:
            ips = sorted({ai[4][0] for ai in socket.getaddrinfo(h, 443, socket.AF_INET)})
        except OSError as e:
            out.append({"venue": venue, "host": h, "error": str(e)})
            continue
        rtts = []
        for i in range(N_TCP):
            r = tcp_rtt(ips[i % len(ips)])
            if r is not None:
                rtts.append(r)
            time.sleep(0.2)
        tls = []
        ver = None
        for i in range(N_TLS):
            d, ver = tls_ms(h, ips[0])
            if d is not None:
                tls.append(d)
            time.sleep(0.3)
        cname = socket.getfqdn(ips[0])
        out.append({
            "venue": venue, "host": h, "ips": ips[:6], "n_ips": len(ips), "rdns": cname,
            "tcp_min_ms": round(min(rtts), 3) if rtts else None,
            "tcp_p50_ms": round(statistics.median(rtts), 3) if rtts else None,
            "tcp_max_ms": round(max(rtts), 3) if rtts else None,
            "tls_min_ms": round(min(tls), 1) if tls else None,
            "tls_p50_ms": round(statistics.median(tls), 1) if tls else None,
            "tls_ver": ver,
        })
        print(json.dumps(out[-1], ensure_ascii=False), flush=True)

json.dump(out, open(sys.argv[1] if len(sys.argv) > 1 else "net_probe.json", "w"), indent=1)
