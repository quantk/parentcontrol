use jiff::civil::date;
use parentcontrol_core::rules::BuiltinProfile;

use super::*;
use crate::db::{DEFAULT_PROFILE, Db};

fn day_14() -> BudgetDay {
    BudgetDay::new(date(2026, 11, 14))
}

#[tokio::test]
async fn group_lifecycle() {
    let db = Db::in_memory().unwrap();
    db.call(|c| {
        let games = create(c, DEFAULT_PROFILE, "Игры")?;
        add_rule(c, games, &Rule::Builtin(BuiltinProfile::MinecraftJava))?;
        assert!(set_blocked(
            c,
            games,
            Some("2026-11-14T10:00:00Z".parse().unwrap())
        )?);

        let groups = list(c, DEFAULT_PROFILE)?;
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "Игры");
        assert_eq!(groups[0].schedule.monday.quota(), DEFAULT_QUOTA);
        assert_eq!(groups[0].rules.len(), 1);
        assert!(groups[0].blocked_since.is_some());

        assert!(delete_rule(c, groups[0].rules[0].id)?);
        assert!(rename(c, games, "Игрушки")?);
        assert!(delete(c, games)?);
        assert_eq!(list(c, DEFAULT_PROFILE)?.len(), 0);
        assert!(!rename(c, games, "нет такой")?);
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn adjustments_are_filtered_by_day() {
    let db = Db::in_memory().unwrap();
    db.call(|c| {
        let games = create(c, DEFAULT_PROFILE, "Игры")?;
        let yesterday = BudgetDay::new(day_14().date().yesterday().unwrap());
        add_adjustment(
            c,
            games,
            Adjustment {
                day: yesterday,
                minutes: MinuteDelta::new(-5),
            },
        )?;
        add_adjustment(
            c,
            games,
            Adjustment {
                day: day_14(),
                minutes: MinuteDelta::new(15),
            },
        )?;
        let adjustments = adjustments_since(c, day_14())?;
        assert_eq!(
            adjustments,
            [(
                games,
                Adjustment {
                    day: day_14(),
                    minutes: MinuteDelta::new(15)
                }
            )]
        );
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn deleting_a_group_removes_its_rules_and_adjustments() {
    let db = Db::in_memory().unwrap();
    db.call(|c| {
        let games = create(c, DEFAULT_PROFILE, "Игры")?;
        add_rule(c, games, &Rule::Builtin(BuiltinProfile::MinecraftJava))?;
        add_adjustment(
            c,
            games,
            Adjustment {
                day: day_14(),
                minutes: MinuteDelta::new(15),
            },
        )?;
        delete(c, games)?;
        let rules: i64 = c.query_row("SELECT count(*) FROM rules", [], |row| row.get(0))?;
        assert_eq!(rules, 0);
        assert_eq!(adjustments_since(c, day_14())?, []);
        Ok(())
    })
    .await
    .unwrap();
}
