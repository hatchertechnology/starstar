//! Read-only print library tools.
use crate::{
    library,
    mcp::{
        StarstarMcp,
        results::{caller_of, outcome, structured_ok},
    },
};
use axum::http::request::Parts;
use rmcp::{
    ErrorData,
    handler::server::{common::Extension, wrapper::Parameters},
    model::CallToolResult,
    schemars::JsonSchema,
    tool, tool_router,
};
use serde::Deserialize;
use starstar_shared::{LibraryQuery, LibrarySort};

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct ListArgs {
    /// Filter names by this text.
    query: Option<String>,
    /// Sort by newest, name, or sliced_for.
    sort: Option<Sort>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(crate = "rmcp::schemars")]
enum Sort {
    Newest,
    Name,
    SlicedFor,
}
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct InfoArgs {
    /// Library file ID.
    id: i64,
}

#[tool_router(router = library_tools, vis = "pub(super)")]
impl StarstarMcp {
    #[tool(
        description = "Lists sliced files in the print library.",
        annotations(read_only_hint = true)
    )]
    async fn library_list(
        &self,
        Parameters(args): Parameters<ListArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        let sort = args.sort.map(|s| match s {
            Sort::Newest => LibrarySort::Newest,
            Sort::Name => LibrarySort::Name,
            Sort::SlicedFor => LibrarySort::SlicedFor,
        });
        Ok(outcome(
            library::list_files(
                &self.state,
                &caller,
                LibraryQuery {
                    q: args.query,
                    sort,
                },
            )
            .await,
            |v| structured_ok("files", v),
        ))
    }
    #[tool(
        description = "Shows one file in the print library.",
        annotations(read_only_hint = true)
    )]
    async fn library_info(
        &self,
        Parameters(args): Parameters<InfoArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            library::get_file(&self.state, &caller, args.id).await,
            |v| structured_ok("file", v),
        ))
    }
}
