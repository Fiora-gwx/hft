"""az-probe: AZ / CPG latency probe to exchange endpoints (tools/README.md "az-probe").

`az-probe run` measures, for every A record of every target host, the TCP handshake RTT (9 samples
per IP and cycle), the TLS handshake time and, with `--ws-first-frame`, the one-way arrival of
public market-data frames; `az-probe compare` ranks (AZ, CPG) by the sum of the core exchanges'
handshake minRTT. No AWS API call, no API key, no private stream.
"""

__all__ = ["attribution", "cli", "compare", "probe", "stats"]
