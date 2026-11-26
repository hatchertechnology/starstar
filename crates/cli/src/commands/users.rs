//! `user …` and `apps …` commands.

use serde_json::json;
use starstar_shared::auth::{
    AppGrant, ChangePassword, NewUser, UpdateUser, UserSummary, is_valid_username,
};

use super::{Ctx, check_password, read_secret, stdin_line};
use crate::args::{AppsCommand, UserCommand};
use crate::confirm::confirm_here;
use crate::credentials::{CredentialStore, default_path, server_key};
use crate::endpoints;
use crate::error::{CliError, CliResult};
use crate::session::TokenOrigin;

fn check_username(name: &str) -> CliResult<()> {
    if is_valid_username(name) {
        Ok(())
    } else {
        Err(CliError::Usage(format!(
            "invalid username `{name}`: use 1-32 lowercase letters, digits, '.', '_' or '-', starting with a letter or digit"
        )))
    }
}

/// `YYYY-MM-DD` (UTC) for Unix seconds.
pub fn format_date(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn summary_line(u: &UserSummary) -> String {
    format!(
        "{:<20} {:<8} {}",
        u.username,
        u.role.as_str(),
        if u.disabled { "disabled" } else { "enabled" }
    )
}

fn patch(ctx: &Ctx, username: &str, update: UpdateUser) -> CliResult<()> {
    check_username(username)?;
    let u: UserSummary = ctx
        .client()?
        .send_json(&endpoints::update_user(username), &update)?;
    ctx.out.emit_value(&u, || summary_line(&u));
    Ok(())
}

pub fn run_user(ctx: &Ctx, command: UserCommand) -> CliResult<()> {
    let out = ctx.out;
    match command {
        UserCommand::Add {
            username,
            role,
            password_stdin,
        } => {
            check_username(&username)?;
            let password = if password_stdin {
                stdin_line()?
            } else {
                let p = read_secret(&format!("Password for {username}: "), false)?;
                if read_secret("Repeat: ", false)? != p {
                    return Err(CliError::Usage("passwords did not match".into()));
                }
                p
            };
            check_password(&password)?;
            let u: UserSummary = ctx.client()?.send_json(
                &endpoints::add_user(),
                &NewUser {
                    username,
                    password,
                    role,
                },
            )?;
            out.emit_value(&u, || format!("created {}", summary_line(&u)));
        }
        UserCommand::List => {
            let users: Vec<UserSummary> = ctx.client()?.get_json(&endpoints::users())?;
            out.emit_value(&users, || {
                users
                    .iter()
                    .map(summary_line)
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        UserCommand::SetRole { username, role } => patch(
            ctx,
            &username,
            UpdateUser {
                role: Some(role),
                disabled: None,
            },
        )?,
        UserCommand::Disable { username } => patch(
            ctx,
            &username,
            UpdateUser {
                role: None,
                disabled: Some(true),
            },
        )?,
        UserCommand::Enable { username } => patch(
            ctx,
            &username,
            UpdateUser {
                role: None,
                disabled: Some(false),
            },
        )?,
        UserCommand::Remove { username, confirm } => {
            check_username(&username)?;
            confirm_here(&format!("remove user `{username}`"), confirm.yes)?;
            ctx.client()?
                .send_empty(&endpoints::remove_user(&username), None)?;
            out.emit(&json!({"username": username, "action": "remove"}), || {
                format!("removed user `{username}`")
            });
        }
        UserCommand::Passwd { password_stdin } => passwd(ctx, password_stdin)?,
    }
    Ok(())
}

fn passwd(ctx: &Ctx, password_stdin: bool) -> CliResult<()> {
    let (current, new) = if password_stdin {
        (stdin_line()?, stdin_line()?)
    } else {
        let current = read_secret("Current password: ", false)?;
        let new = read_secret("New password: ", false)?;
        if read_secret("Repeat: ", false)? != new {
            return Err(CliError::Usage("passwords did not match".into()));
        }
        (current, new)
    };
    check_password(&new)?;
    let client = ctx.client()?;
    client.send_empty(
        &endpoints::my_password(),
        Some(
            &serde_json::to_value(ChangePassword {
                current_password: current,
                new_password: new,
            })
            .map_err(|e| CliError::Other(e.to_string()))?,
        ),
    )?;
    // The server revoked every grant of this user, ours included.
    let env = client.tokens().map(|t| t.origin()) == Some(TokenOrigin::Env);
    if !env {
        CredentialStore::at(default_path()?).remove(&server_key(&ctx.api))?;
    }
    ctx.out
        .emit(&json!({"action": "passwd", "signed_out": true}), || {
            let mut text = "Password changed. Your other sessions and app sign-ins were signed out; run `starstar login` to sign in again.".to_owned();
            if env {
                text.push_str("\nnote: the token in STARSTAR_TOKEN was revoked too.");
            }
            text
        });
    Ok(())
}

pub fn run_apps(ctx: &Ctx, command: AppsCommand) -> CliResult<()> {
    match command {
        AppsCommand::List { all } => {
            let apps: Vec<AppGrant> = ctx.client()?.get_json(&endpoints::apps(all))?;
            ctx.out.emit_value(&apps, || {
                if apps.is_empty() {
                    return "no connected apps".to_owned();
                }
                apps.iter()
                    .map(|a| {
                        format!(
                            "{} {} {} {} last used {}",
                            a.id,
                            a.client_name,
                            a.username,
                            a.role,
                            a.last_used_at
                                .map_or_else(|| "never".to_owned(), format_date)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        AppsCommand::Revoke { id } => {
            endpoints::validate_app_id(&id)?;
            ctx.client()?
                .send_empty(&endpoints::revoke_app(&id), None)?;
            ctx.out.emit(&json!({"id": id, "action": "revoke"}), || {
                format!("revoked app {id}")
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_dates() {
        assert_eq!(format_date(1_800_000_000), "2027-01-15");
        assert_eq!(format_date(0), "1970-01-01");
        assert_eq!(format_date(951_782_400), "2000-02-29");
    }
}
