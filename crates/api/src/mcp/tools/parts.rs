//! Parts library discovery and matching.
use axum::http::request::Parts;
use rmcp::handler::server::{common::Extension, wrapper::Parameters};
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::{ErrorData, tool, tool_router};
use serde::Deserialize;
use starstar_shared::PartSort;

use crate::error::ApiError;
use crate::library::{EDIT_PARTS_ROLE, READ_PARTS_ROLE};
use crate::mcp::StarstarMcp;
use crate::mcp::results::{caller_of, outcome, structured_ok, tool_try};

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct ListArgs {
    /// Search part names, file names and profiles.
    query: Option<String>,
    /// Sort by newest (default) or name.
    sort: Option<Sort>,
}
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(rename_all = "snake_case")]
enum Sort {
    Newest,
    Name,
}
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct PartArgs {
    /// Part ID.
    id: i64,
}
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct MatchArgs {
    /// Part ID.
    id: i64,
    /// Setup ID; defaults to the part's default setup.
    setup_id: Option<i64>,
    /// Profile; defaults to the setup's default profile.
    profile: Option<String>,
}

#[tool_router(router = part_tools, vis = "pub(super)")]
impl StarstarMcp {
    #[tool(
        description = "Lists parts, optionally searching and sorting them.",
        annotations(read_only_hint = true)
    )]
    async fn part_list(
        &self,
        Parameters(args): Parameters<ListArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        tool_try!(caller.require(READ_PARTS_ROLE));
        let sort = match args.sort {
            Some(Sort::Name) => PartSort::Name,
            _ => PartSort::Newest,
        };
        Ok(outcome(
            self.state
                .parts
                .list(args.query.as_deref(), sort)
                .await
                .map_err(Into::into),
            |v| structured_ok("parts", v),
        ))
    }
    #[tool(
        description = "Shows a part's complete revision and setup tree.",
        annotations(read_only_hint = true)
    )]
    async fn part_info(
        &self,
        Parameters(args): Parameters<PartArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        tool_try!(caller.require(READ_PARTS_ROLE));
        let result = self
            .state
            .parts
            .tree(args.id)
            .await
            .map_err(ApiError::from)
            .and_then(|p| p.ok_or_else(|| ApiError::NotFound("part not found".into())));
        Ok(outcome(result, |v| structured_ok("part", v)))
    }
    #[tool(
        description = "Matches a part's setup and profile to registered printers.",
        annotations(read_only_hint = true)
    )]
    async fn part_match(
        &self,
        Parameters(args): Parameters<MatchArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        tool_try!(caller.require(EDIT_PARTS_ROLE));
        Ok(outcome(
            crate::routes::parts::match_for_part(
                &self.state,
                args.id,
                args.setup_id,
                args.profile.as_deref(),
            )
            .await,
            |v| structured_ok("matches", v),
        ))
    }
}
