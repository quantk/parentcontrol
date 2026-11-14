//! Groups and their rules.

use askama::Template;
use axum::Form;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use parentcontrol_core::ids::GroupId;
use parentcontrol_core::rules::{BuiltinProfile, Rule};
use serde::Deserialize;
use serde_json::json;

use super::{Flash, Icon, Layout, back, done, rejected, render, valid_name};
use crate::auth::Parent;
use crate::db::{DEFAULT_PROFILE, groups};
use crate::error::AppError;
use crate::state::AppState;

const PAGE: &str = "/groups";

struct RuleRow {
    id: i64,
    text: String,
}

struct GroupRow {
    id: GroupId,
    name: String,
    rules: Vec<RuleRow>,
}

#[derive(Template)]
#[template(path = "groups.html")]
struct GroupsPage {
    layout: Layout,
    groups: Vec<GroupRow>,
}

pub(super) async fn page(
    State(state): State<AppState>,
    _: Parent,
    flash: Option<Flash>,
) -> Result<Response, AppError> {
    let stored = state.db.call(|c| groups::list(c, DEFAULT_PROFILE)).await?;
    let groups = stored
        .into_iter()
        .map(|group| GroupRow {
            id: group.id,
            name: group.name,
            rules: group
                .rules
                .into_iter()
                .map(|stored| RuleRow {
                    id: stored.id,
                    text: describe(&stored.rule),
                })
                .collect(),
        })
        .collect();
    let page = GroupsPage {
        layout: Layout {
            title: "Группы",
            active: PAGE,
            flash,
        },
        groups,
    };
    Ok(render(&page)?.into_response())
}

/// The rule as the parent reads it.
fn describe(rule: &Rule) -> String {
    match rule {
        Rule::Builtin(BuiltinProfile::MinecraftJava) => {
            "Minecraft Java Edition (встроенный профиль)".to_owned()
        }
        Rule::Executable(name) => format!("Исполняемый файл: {}", name.as_str()),
        Rule::PathGlob(glob) => format!("Путь: {}", glob.as_str()),
        Rule::CommandLineContains(text) => format!("Командная строка содержит: {}", text.as_str()),
        Rule::CommandLineRegex(regex) => {
            format!("Командная строка, регулярное выражение: {}", regex.as_str())
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct NameForm {
    name: String,
}

pub(super) async fn create(
    State(state): State<AppState>,
    _: Parent,
    Form(form): Form<NameForm>,
) -> Result<Response, AppError> {
    let name = match valid_name(&form.name) {
        Ok(name) => name,
        Err(error) => return Ok(rejected(PAGE, error)),
    };
    let stored = name.clone();
    state
        .db
        .call(move |c| groups::create(c, DEFAULT_PROFILE, &stored))
        .await?;
    done(&state, PAGE, format!("Создана группа: {name}")).await
}

pub(super) async fn rename(
    State(state): State<AppState>,
    _: Parent,
    Path(group): Path<GroupId>,
    Form(form): Form<NameForm>,
) -> Result<Response, AppError> {
    let name = match valid_name(&form.name) {
        Ok(name) => name,
        Err(error) => return Ok(rejected(PAGE, error)),
    };
    let stored = name.clone();
    let old = state
        .db
        .call(move |c| {
            let old = groups::name(c, group)?;
            groups::rename(c, group, &stored)?;
            Ok(old)
        })
        .await?;
    if let Some(old) = old {
        return done(
            &state,
            PAGE,
            format!("Группа «{old}» переименована в «{name}»"),
        )
        .await;
    }
    Ok(back(PAGE))
}

pub(super) async fn delete(
    State(state): State<AppState>,
    _: Parent,
    Path(group): Path<GroupId>,
) -> Result<Response, AppError> {
    let name = state
        .db
        .call(move |c| {
            let name = groups::name(c, group)?;
            groups::delete(c, group)?;
            Ok(name)
        })
        .await?;
    if let Some(name) = name {
        return done(&state, PAGE, format!("Удалена группа: {name}")).await;
    }
    Ok(back(PAGE))
}

#[derive(Debug, Deserialize)]
pub(super) struct RuleForm {
    kind: String,
    #[serde(default)]
    value: String,
}

/// Builds a rule from the form, reusing the validation of its JSON form.
fn parse_rule(form: &RuleForm) -> Result<Rule, String> {
    let value = form.value.trim();
    let json = match form.kind.as_str() {
        "minecraft_java" => json!({ "builtin": "minecraft_java" }),
        kind @ ("executable" | "path_glob" | "command_line_contains" | "command_line_regex") => {
            json!({ kind: value })
        }
        _ => return Err("Неизвестный тип правила.".to_owned()),
    };
    serde_json::from_value(json).map_err(|error| format!("Правило не принято: {error}"))
}

pub(super) async fn add_rule(
    State(state): State<AppState>,
    _: Parent,
    Path(group): Path<GroupId>,
    Form(form): Form<RuleForm>,
) -> Result<Response, AppError> {
    let rule = match parse_rule(&form) {
        Ok(rule) => rule,
        Err(error) => return Ok(rejected(PAGE, &error)),
    };
    let text = describe(&rule);
    let name = state
        .db
        .call(move |c| {
            let name = groups::name(c, group)?;
            if name.is_some() {
                groups::add_rule(c, group, &rule)?;
            }
            Ok(name)
        })
        .await?;
    if let Some(name) = name {
        return done(
            &state,
            PAGE,
            format!("Группа «{name}»: добавлено правило «{text}»"),
        )
        .await;
    }
    Ok(back(PAGE))
}

pub(super) async fn delete_rule(
    State(state): State<AppState>,
    _: Parent,
    Path(rule): Path<i64>,
) -> Result<Response, AppError> {
    if state.db.call(move |c| groups::delete_rule(c, rule)).await? {
        return done(&state, PAGE, "Удалено правило".to_owned()).await;
    }
    Ok(back(PAGE))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(kind: &str, value: &str) -> RuleForm {
        RuleForm {
            kind: kind.to_owned(),
            value: value.to_owned(),
        }
    }

    #[test]
    fn builds_every_kind_of_rule() {
        assert_eq!(
            parse_rule(&form("minecraft_java", "")),
            Ok(Rule::Builtin(BuiltinProfile::MinecraftJava))
        );
        for kind in [
            "executable",
            "path_glob",
            "command_line_contains",
            "command_line_regex",
        ] {
            assert!(parse_rule(&form(kind, "tetris")).is_ok(), "{kind}");
        }
    }

    #[test]
    fn invalid_rules_are_explained() {
        assert!(parse_rule(&form("executable", "")).is_err());
        assert!(parse_rule(&form("command_line_regex", "(")).is_err());
        assert!(parse_rule(&form("format_disk", "x")).is_err());
    }
}
