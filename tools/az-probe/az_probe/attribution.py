"""Endpoint attribution: direct EC2, CloudFront, Cloudflare or other, from AWS `ip-ranges.json` and
the Cloudflare IPv4 list (design/03 §3.5)."""

from __future__ import annotations

import ipaddress
import json
from dataclasses import dataclass, field

AWS_RANGES_URL = "https://ip-ranges.amazonaws.com/ip-ranges.json"
CLOUDFLARE_URL = "https://www.cloudflare.com/ips-v4"


@dataclass
class Ranges:
    """Parsed prefix lists."""

    ec2: list[ipaddress.IPv4Network] = field(default_factory=list)
    cloudfront: list[ipaddress.IPv4Network] = field(default_factory=list)
    cloudflare: list[ipaddress.IPv4Network] = field(default_factory=list)

    @staticmethod
    def parse(aws_json: str | None, cloudflare_txt: str | None) -> Ranges | None:
        """`None` when neither list is available (attribution `unknown`)."""
        if aws_json is None and cloudflare_txt is None:
            return None
        r = Ranges()
        if aws_json is not None:
            for p in json.loads(aws_json).get("prefixes", []):
                net = ipaddress.IPv4Network(p["ip_prefix"])
                if p.get("service") == "EC2":
                    r.ec2.append(net)
                elif p.get("service") == "CLOUDFRONT":
                    r.cloudfront.append(net)
        if cloudflare_txt is not None:
            for line in cloudflare_txt.splitlines():
                line = line.strip()
                if line and not line.startswith("#"):
                    r.cloudflare.append(ipaddress.IPv4Network(line))
        return r

    def classify(self, ip: str) -> str:
        """`cloudfront`, `ec2`, `cloudflare` or `other` (CloudFront first: AWS lists its prefixes
        under several services)."""
        a = ipaddress.IPv4Address(ip)
        if any(a in n for n in self.cloudfront):
            return "cloudfront"
        if any(a in n for n in self.ec2):
            return "ec2"
        if any(a in n for n in self.cloudflare):
            return "cloudflare"
        return "other"


def classify(ranges: Ranges | None, ip: str) -> str:
    """Attribution of `ip`; `unknown` without range lists."""
    return "unknown" if ranges is None else ranges.classify(ip)
