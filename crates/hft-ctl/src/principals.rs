//! Issuer table (`paths.ctl_principals`): `[uids]` maps system users to principal names,
//! `[issuers]` maps `tg:<id>` and `cli:<login>` to role sets. Loaded once at startup.

use std::path::Path;

use hft_ctl_proto::{Principal, Role, RoleSet};

/// Load errors.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum PrincipalsErr {
    Io(String),
    Parse(String),
    UnknownUser(String),
    UnknownPrincipal(String),
    UnknownRole(String),
    BadIssuer(String),
    DupEntry(String),
}

/// What a uid may present as.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum UidKind {
    /// `hft-opsbot`: `tg:<id>` and `ops_bot`.
    OpsBot,
    Pipeline,
    Reconciler,
    /// An operator login: only `cli:<login>`.
    Cli {
        login: String,
    },
}

/// The loaded table.
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct PrincipalTable {
    uids: Vec<(u32, UidKind)>,
    issuers: Vec<(String, RoleSet)>,
}

/// Result of resolving `(uid, issuer)`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Resolved {
    pub principal: Principal,
    /// Role set of a human issuer; empty for process principals.
    pub roles: RoleSet,
}

fn valid_issuer(s: &str) -> bool {
    if let Some(id) = s.strip_prefix("tg:") {
        return !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit());
    }
    s.strip_prefix("cli:")
        .is_some_and(|l| !l.is_empty() && l != "systemd")
}

impl PrincipalTable {
    /// Builds a table directly (tests and assembly helpers).
    pub fn from_parts(uids: Vec<(u32, UidKind)>, issuers: Vec<(String, RoleSet)>) -> PrincipalTable {
        PrincipalTable { uids, issuers }
    }

    /// Loads the TOML file; user names resolve through the system user database.
    pub fn load(path: &Path) -> Result<PrincipalTable, PrincipalsErr> {
        PrincipalTable::load_with(path, sys_uid)
    }

    /// Loads with an injected user-name → uid resolver.
    pub fn load_with(
        path: &Path,
        uid_of: impl Fn(&str) -> Option<u32>,
    ) -> Result<PrincipalTable, PrincipalsErr> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| PrincipalsErr::Io(format!("{}: {e}", path.display())))?;
        PrincipalTable::parse(&text, uid_of)
    }

    /// Parses the TOML text.
    pub fn parse(text: &str, uid_of: impl Fn(&str) -> Option<u32>) -> Result<PrincipalTable, PrincipalsErr> {
        // The TOML parser rejects duplicate keys; report them as DupEntry.
        let t: toml::Table = text.parse().map_err(|e: toml::de::Error| {
            if e.message().contains("duplicate") {
                PrincipalsErr::DupEntry(e.message().to_owned())
            } else {
                PrincipalsErr::Parse(e.message().to_owned())
            }
        })?;
        for k in t.keys() {
            if k != "uids" && k != "issuers" {
                return Err(PrincipalsErr::Parse(format!("unknown key {k}")));
            }
        }
        let table = |k: &str| -> Result<toml::Table, PrincipalsErr> {
            match t.get(k) {
                None => Ok(toml::Table::new()),
                Some(toml::Value::Table(x)) => Ok(x.clone()),
                Some(_) => Err(PrincipalsErr::Parse(format!("{k} is not a table"))),
            }
        };
        let mut uids = Vec::new();
        let mut logins = Vec::new();
        for (user, v) in table("uids")? {
            let name = v
                .as_str()
                .ok_or_else(|| PrincipalsErr::Parse(format!("uids.{user} is not a string")))?;
            let kind = match name {
                "ops_bot" => UidKind::OpsBot,
                "pipeline" => UidKind::Pipeline,
                "reconciler" => UidKind::Reconciler,
                "cli" => {
                    logins.push(user.clone());
                    UidKind::Cli { login: user.clone() }
                }
                other => return Err(PrincipalsErr::UnknownPrincipal(other.to_owned())),
            };
            let uid = uid_of(&user).ok_or_else(|| PrincipalsErr::UnknownUser(user.clone()))?;
            if uids.iter().any(|(u, _)| *u == uid) {
                return Err(PrincipalsErr::DupEntry(user));
            }
            uids.push((uid, kind));
        }
        let mut issuers = Vec::new();
        for (issuer, v) in table("issuers")? {
            if !valid_issuer(&issuer) {
                return Err(PrincipalsErr::BadIssuer(issuer));
            }
            if let Some(login) = issuer.strip_prefix("cli:")
                && !logins.iter().any(|l| l == login)
            {
                return Err(PrincipalsErr::BadIssuer(issuer));
            }
            let arr = v
                .as_array()
                .ok_or_else(|| PrincipalsErr::Parse(format!("issuers.{issuer} is not an array")))?;
            let mut set = RoleSet::EMPTY;
            for r in arr {
                let name = r.as_str().unwrap_or("");
                let role =
                    Role::from_name(name).ok_or_else(|| PrincipalsErr::UnknownRole(name.to_owned()))?;
                set = set.with(role);
            }
            issuers.push((issuer, set));
        }
        Ok(PrincipalTable { uids, issuers })
    }

    /// The uid entry; unknown → `None`.
    pub fn uid(&self, uid: u32) -> Option<&UidKind> {
        self.uids.iter().find(|(u, _)| *u == uid).map(|(_, k)| k)
    }

    /// Role set of a human issuer.
    pub fn roles(&self, issuer: &str) -> Option<RoleSet> {
        self.issuers.iter().find(|(i, _)| i == issuer).map(|(_, r)| *r)
    }

    /// Authentication step 2: the issuer form must belong to the uid, human issuers must be in
    /// `[issuers]`; `cli:systemd` never comes over the socket.
    pub fn resolve(&self, uid: u32, issuer: &str) -> Option<Resolved> {
        let kind = self.uid(uid)?;
        let process = |principal| {
            Some(Resolved {
                principal,
                roles: RoleSet::EMPTY,
            })
        };
        match (kind, issuer) {
            (UidKind::OpsBot, "ops_bot") => process(Principal::OpsBot),
            (UidKind::Pipeline, "pipeline") => process(Principal::Pipeline),
            (UidKind::Reconciler, "reconciler") => process(Principal::Reconciler),
            (UidKind::OpsBot, i) if Principal::of_issuer(i) == Some(Principal::Tg) => Some(Resolved {
                principal: Principal::Tg,
                roles: self.roles(i)?,
            }),
            (UidKind::Cli { login }, i)
                if i.strip_prefix("cli:") == Some(login.as_str()) && login != "systemd" =>
            {
                Some(Resolved {
                    principal: Principal::Cli,
                    roles: self.roles(i)?,
                })
            }
            _ => None,
        }
    }
}

#[cfg(unix)]
fn sys_uid(user: &str) -> Option<u32> {
    let c = std::ffi::CString::new(user).ok()?;
    // SAFETY: `getpwnam` reads the passwd database and returns a pointer to static storage (or
    // null); the uid is copied out before any other call can overwrite it. Startup only.
    unsafe {
        let p = libc::getpwnam(c.as_ptr());
        if p.is_null() { None } else { Some((*p).pw_uid) }
    }
}

#[cfg(not(unix))]
fn sys_uid(_user: &str) -> Option<u32> {
    None
}
