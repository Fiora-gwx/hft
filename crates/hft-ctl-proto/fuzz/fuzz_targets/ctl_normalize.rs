//! CTP-171: any bytes as request JSON; parsed requests go through `normalize` (P0 refdata),
//! `class_of`, `allowed`, `needs_confirm_static` and `check_confirm_fields`.
#![no_main]

use std::sync::OnceLock;

use hft_ctl_proto::{
    ChatType, Principal, RoleSet, allowed, check_confirm_fields, class_of, effective_role,
    needs_confirm_static, normalize, parse_request,
};
use hft_refdata::RefData;
use libfuzzer_sys::fuzz_target;

static P0: &[u8] = include_bytes!("../../../../fixtures/refdata/ctl-p0.blob");

fn rd() -> &'static RefData {
    static RD: OnceLock<RefData> = OnceLock::new();
    RD.get_or_init(|| RefData::load(P0, None).unwrap_or_else(|e| panic!("P0: {e:?}")))
}

fn ctp_171_ctl_normalize(data: &[u8]) {
    let Ok(req) = parse_request(data) else { return };
    let _ = normalize(&req, rd());
    let _ = class_of(&req.cmd);
    let all = RoleSet::VIEWER.union(RoleSet::TRADER).union(RoleSet::ADMIN);
    for p in [
        Principal::Tg,
        Principal::Cli,
        Principal::OpsBot,
        Principal::Pipeline,
        Principal::Reconciler,
        Principal::Systemd,
    ] {
        for chat in [ChatType::Private, ChatType::Group] {
            let _ = allowed(all, p, chat, &req.cmd);
            let role = effective_role(all, p, chat, &req.cmd);
            let rule = needs_confirm_static(role, p, &req.cmd);
            let _ = check_confirm_fields(rule, &req);
        }
    }
}

fuzz_target!(|data: &[u8]| ctp_171_ctl_normalize(data));
