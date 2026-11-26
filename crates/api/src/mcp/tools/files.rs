//! File tools, including the capped base64 upload.

use axum::http::request::Parts;
use rmcp::handler::server::common::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{ErrorData, tool, tool_router};
use starstar_shared::{DeleteOptions, UploadOptions};

use crate::mcp::StarstarMcp;
use crate::mcp::params::{DeleteFileArgs, FileArgs, FilesArgs, UploadArgs};
use crate::mcp::results::{caller_of, decode_upload, outcome, structured_ok, text_ok, tool_try};
use crate::printers::files::{self, UploadBody};

#[tool_router(router = file_tools, vis = "pub(super)")]
impl StarstarMcp {
    #[tool(
        description = "Lists a folder on printer storage, or shows one file with its print metadata. Omit path for the storage root.",
        annotations(read_only_hint = true)
    )]
    async fn list_files(
        &self,
        Parameters(args): Parameters<FilesArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            files::file_info(
                &self.state,
                &caller,
                &args.printer,
                &args.storage,
                args.path.as_deref(),
            )
            .await,
            |v| structured_ok("entry", v),
        ))
    }

    #[tool(
        description = "Tells whether a file is read-only and whether the printer is printing it right now.",
        annotations(read_only_hint = true)
    )]
    async fn file_state(
        &self,
        Parameters(args): Parameters<FileArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            files::file_state(
                &self.state,
                &caller,
                &args.printer,
                &args.storage,
                &args.path,
            )
            .await,
            |v| structured_ok("state", v),
        ))
    }

    #[tool(
        description = "Uploads a file to printer storage. content_base64 is the whole file, base64-encoded; files over the server's MCP upload limit (25 MB by default) must go through the starstar CLI instead.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn upload_file(
        &self,
        Parameters(args): Parameters<UploadArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        // Role first: a caller who may not upload gets 403 before any work.
        tool_try!(caller.require(files::UPLOAD_FILE_ROLE));
        let bytes = tool_try!(decode_upload(
            &args.content_base64,
            self.state.config.mcp_max_upload_bytes
        ));
        let opts = UploadOptions {
            overwrite: args.overwrite,
            print: args.print,
        };
        Ok(outcome(
            files::upload_file(
                &self.state,
                &caller,
                &args.printer,
                &args.storage,
                &args.path,
                UploadBody::from_bytes(bytes),
                opts,
            )
            .await,
            |()| {
                let mut sentence = format!(
                    "Uploaded {} to {}:{}.",
                    args.path, args.printer, args.storage
                );
                if args.print {
                    sentence.push_str(" Printing started.");
                }
                text_ok(sentence)
            },
        ))
    }

    #[tool(
        description = "Starts printing a file that is already on the printer.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn start_print(
        &self,
        Parameters(args): Parameters<FileArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            files::print_file(
                &self.state,
                &caller,
                &args.printer,
                &args.storage,
                &args.path,
            )
            .await,
            |()| {
                text_ok(format!(
                    "Started printing {} on {}.",
                    args.path, args.printer
                ))
            },
        ))
    }

    #[tool(
        description = "Deletes a file or folder from printer storage. Set force to delete a folder even if it is not empty.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn delete_file(
        &self,
        Parameters(args): Parameters<DeleteFileArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            files::delete_file(
                &self.state,
                &caller,
                &args.printer,
                &args.storage,
                &args.path,
                DeleteOptions { force: args.force },
            )
            .await,
            |()| {
                text_ok(format!(
                    "Deleted {} from {}:{}.",
                    args.path, args.printer, args.storage
                ))
            },
        ))
    }
}
