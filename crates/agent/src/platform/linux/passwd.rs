//! Account names from `/etc/passwd`.
//!
//! Accounts that exist only through NSS modules (LDAP, systemd-homed) are
//! not seen; home PCs do not use them.

use std::collections::HashMap;
use std::{fs, io};

use parentcontrol_core::process::OsUser;

/// Account names by uid.
#[derive(Debug, Default)]
pub(super) struct Users(HashMap<u32, OsUser>);

impl Users {
    /// Reads `/etc/passwd`.
    pub(super) fn load() -> io::Result<Self> {
        fs::read_to_string("/etc/passwd").map(|text| Self::parse(&text))
    }

    /// The account with `uid`, if known.
    pub(super) fn name(&self, uid: u32) -> Option<&OsUser> {
        self.0.get(&uid)
    }

    fn parse(text: &str) -> Self {
        let users = text
            .lines()
            .filter_map(|line| {
                let mut fields = line.split(':');
                let name = fields.next().filter(|name| !name.is_empty())?;
                let uid = fields.nth(1)?.parse().ok()?;
                Some((uid, OsUser::new(name)))
            })
            .collect();
        Self(users)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names_and_uids() {
        let users =
            Users::parse("root:x:0:0::/root:/bin/sh\nkid:x:1001:100::/home/kid:/bin/bash\n");
        assert_eq!(users.name(0), Some(&OsUser::new("root")));
        assert_eq!(users.name(1001), Some(&OsUser::new("kid")));
        assert_eq!(users.name(1002), None);
    }

    #[test]
    fn skips_malformed_lines() {
        let users = Users::parse("garbage\n:x:5:5\nbad:x:notanumber:0\n# comment\n");
        assert_eq!(users.0.len(), 0);
    }
}
