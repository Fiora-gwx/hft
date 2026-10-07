"""Endpoint attribution by AWS ip-ranges.json and the Cloudflare list (tools/TESTS.md TOOL-093)."""

from __future__ import annotations

import json

from az_probe.attribution import Ranges


def test_tool_093_endpoint_attribution() -> None:
    aws = json.dumps(
        {
            "prefixes": [
                {"ip_prefix": "3.112.0.0/14", "region": "ap-northeast-1", "service": "AMAZON"},
                {"ip_prefix": "3.112.0.0/14", "region": "ap-northeast-1", "service": "EC2"},
                {"ip_prefix": "13.32.0.0/15", "region": "GLOBAL", "service": "AMAZON"},
                {"ip_prefix": "13.32.0.0/15", "region": "GLOBAL", "service": "CLOUDFRONT"},
            ]
        }
    )
    r = Ranges.parse(aws, "104.16.0.0/13\n")
    assert r is not None
    rows = [
        ("TOOL-093.1", "3.112.5.6", "ec2"),
        ("TOOL-093.2", "13.32.1.1", "cloudfront"),
        ("TOOL-093.3", "104.16.2.2", "cloudflare"),
        ("TOOL-093.4", "8.8.8.8", "other"),
    ]
    for row, ip, want in rows:
        assert r.classify(ip) == want, row
