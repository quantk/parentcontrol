use super::*;
use crate::db::{DEFAULT_PROFILE, Db};

fn at(text: &str) -> Timestamp {
    text.parse().unwrap()
}

#[tokio::test]
async fn enrollment_consumes_the_code_and_issues_a_token() {
    let db = Db::in_memory().unwrap();
    db.call(|c| {
        let device = create(c, "ПК", "code-hash", at("2026-11-14T10:15:00Z"))?;
        let listed = &list(c)?[0];
        assert!(!listed.enrolled);

        let enrolled = enroll(
            c,
            "code-hash",
            "token-hash",
            DevicePlatform::Linux,
            at("2026-11-14T10:00:00Z"),
        )?;
        assert_eq!(enrolled, Some(device));
        assert_eq!(authenticate(c, "token-hash")?, Some(device));

        let again = enroll(
            c,
            "code-hash",
            "other",
            DevicePlatform::Linux,
            at("2026-11-14T10:01:00Z"),
        )?;
        assert_eq!(again, None, "codes are single-use");

        let listed = &list(c)?[0];
        assert!(listed.enrolled);
        assert_eq!(listed.platform, Some(DevicePlatform::Linux));
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn expired_code_is_refused() {
    let db = Db::in_memory().unwrap();
    db.call(|c| {
        create(c, "ПК", "code-hash", at("2026-11-14T10:15:00Z"))?;
        let enrolled = enroll(
            c,
            "code-hash",
            "token",
            DevicePlatform::Windows,
            at("2026-11-14T10:16:00Z"),
        )?;
        assert_eq!(enrolled, None);
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn reenrollment_replaces_the_old_token() {
    let db = Db::in_memory().unwrap();
    db.call(|c| {
        let device = create(c, "ПК", "first", at("2026-11-14T10:15:00Z"))?;
        enroll(
            c,
            "first",
            "old-token",
            DevicePlatform::Linux,
            at("2026-11-14T10:00:00Z"),
        )?;
        reissue_code(c, device, "second", at("2026-11-14T11:15:00Z"))?;
        enroll(
            c,
            "second",
            "new-token",
            DevicePlatform::Linux,
            at("2026-11-14T11:00:00Z"),
        )?;
        assert_eq!(authenticate(c, "old-token")?, None);
        assert_eq!(authenticate(c, "new-token")?, Some(device));
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn hello_and_bindings_are_stored() {
    let db = Db::in_memory().unwrap();
    db.call(|c| {
        let device = create(c, "ПК", "code", at("2026-11-14T10:15:00Z"))?;
        let users = [OsUser::new("kid"), OsUser::new("parent")];
        record_hello(
            c,
            device,
            DevicePlatform::Windows,
            &users,
            at("2026-11-14T10:05:00Z"),
        )?;
        bind(c, device, &OsUser::new("kid"), DEFAULT_PROFILE)?;
        bind(c, device, &OsUser::new("kid"), DEFAULT_PROFILE)?;

        let listed = &list(c)?[0];
        assert_eq!(listed.local_users, users);
        assert_eq!(listed.bindings, BTreeSet::from([OsUser::new("kid")]));
        assert_eq!(listed.last_seen, Some(at("2026-11-14T10:05:00Z")));

        assert!(unbind(c, device, &OsUser::new("kid"))?);
        assert_eq!(bindings(c, device)?.len(), 0);
        assert!(delete(c, device)?);
        assert_eq!(list(c)?.len(), 0);
        Ok(())
    })
    .await
    .unwrap();
}
