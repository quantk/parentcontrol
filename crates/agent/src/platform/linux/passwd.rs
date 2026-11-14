//! Account names from `/etc/passwd`.
//!
//! Accounts that exist only through NSS modules (LDAP, systemd-homed) are
//! not seen; home PCs do not use them.

use std::collections::HashMap;
use std::{fs, io};

use parentcontrol_core::process::OsUser;

/// Uids of human accounts: from 1000 up, below the range systemd and
/// distributions reserve for containers and dynamic users.
const PEOPLE: std::ops::Range<u32> = 1000..60000;

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

    /// Accounts in the range distributions give to people, sorted by name.
    pub(super) fn people(&self) -> Vec<OsUser> {
        let mut people: Vec<OsUser> = self
            .0
            .iter()
            .filter(|(uid, _)| PEOPLE.contains(*uid))
            .map(|(_, user)| user.clone())
            .collect();
        people.sort();
        people
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
    fn people_exclude_system_accounts() {
        let users = Users::parse(
            "root:x:0:0::/:/bin/sh\nnobody:x:65534:65534::/:/bin/false\nkid:x:1001:100::/:/bin/sh\nmom:x:1000:100::/:/bin/sh\n",
        );
        assert_eq!(users.people(), [OsUser::new("kid"), OsUser::new("mom")]);
    }

    #[test]
    fn skips_malformed_lines() {
        let users = Users::parse("garbage\n:x:5:5\nbad:x:notanumber:0\n# comment\n");
        assert_eq!(users.0.len(), 0);
    }
}
