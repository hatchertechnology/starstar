//! Roles of every printer operation and route.
//!
//! The direct-call tests run today. The two HTTP tests that need real
//! authentication are ignored until the auth part lands.

use std::future::Future;
use std::pin::Pin;

use axum::body::Body;
use axum::http::{Method, StatusCode};
use serde_json::json;
use starstar_api::AppState;
use starstar_api::auth::Caller;
use starstar_api::error::ApiError;
use starstar_api::printers::files::UploadBody;
use starstar_api::printers::{cameras, files, jobs, registry, telemetry, transfer, update};
use starstar_shared::{
    CameraSettings, DeleteOptions, PrinterKind, RegisterPrinter, Role, UploadOptions,
};

mod common;
mod fake_prusalink;

use fake_prusalink::{Canned, PASSWORD, TestApi, TestResponse, USERNAME};

type Fut = Pin<Box<dyn Future<Output = Result<(), ApiError>>>>;
type Op = Box<dyn Fn(AppState, Caller) -> Fut>;

fn op<T, F, Fu>(f: F) -> Op
where
    F: Fn(AppState, Caller) -> Fu + 'static,
    Fu: Future<Output = Result<T, ApiError>> + 'static,
{
    Box::new(move |s, c| {
        let fut = f(s, c);
        Box::pin(async move { fut.await.map(|_| ()) })
    })
}

fn one_below(r: Role) -> Role {
    match r {
        Role::Admin => Role::Operator,
        _ => Role::Viewer,
    }
}

fn caller(role: Role) -> Caller {
    Caller {
        username: "tester".into(),
        role,
    }
}

/// Every `printers::` operation: its name, the role the spec gives it, the
/// role const next to it, and a call that exercises it against the fake.
fn operations(fake_host: String) -> Vec<(&'static str, Role, Role, Op)> {
    const P: &str = "mk4";
    const F: &str = "cube-12mm.bgcode";
    let register = move |s: AppState, c: Caller| {
        let host = fake_host.clone();
        async move {
            let req = RegisterPrinter {
                name: "newp".into(),
                kind: PrinterKind::Mk4,
                host,
                username: USERNAME.into(),
                password: PASSWORD.into(),
            };
            registry::register_printer(&s, &c, req).await
        }
    };
    vec![
        (
            "list_printers",
            Role::Viewer,
            registry::LIST_PRINTERS_ROLE,
            op(|s, c| async move { registry::list_printers(&s, &c).await }),
        ),
        (
            "show_printer",
            Role::Viewer,
            registry::SHOW_PRINTER_ROLE,
            op(|s, c| async move { registry::show_printer(&s, &c, P).await }),
        ),
        (
            "register_printer",
            Role::Admin,
            registry::REGISTER_PRINTER_ROLE,
            op(register),
        ),
        (
            "remove_printer",
            Role::Admin,
            registry::REMOVE_PRINTER_ROLE,
            op(|s, c| async move { registry::remove_printer(&s, &c, P).await }),
        ),
        (
            "printer_info",
            Role::Viewer,
            telemetry::PRINTER_INFO_ROLE,
            op(|s, c| async move { telemetry::printer_info(&s, &c, P).await }),
        ),
        (
            "printer_status",
            Role::Viewer,
            telemetry::PRINTER_STATUS_ROLE,
            op(|s, c| async move { telemetry::printer_status(&s, &c, P).await }),
        ),
        (
            "printer_storage",
            Role::Viewer,
            telemetry::PRINTER_STORAGE_ROLE,
            op(|s, c| async move { telemetry::printer_storage(&s, &c, P).await }),
        ),
        (
            "current_job",
            Role::Viewer,
            jobs::CURRENT_JOB_ROLE,
            op(|s, c| async move { jobs::current_job(&s, &c, P).await }),
        ),
        (
            "stop_job",
            Role::Operator,
            jobs::STOP_JOB_ROLE,
            op(|s, c| async move { jobs::stop_job(&s, &c, P, 42).await }),
        ),
        (
            "pause_job",
            Role::Operator,
            jobs::PAUSE_JOB_ROLE,
            op(|s, c| async move { jobs::pause_job(&s, &c, P, 42).await }),
        ),
        (
            "resume_job",
            Role::Operator,
            jobs::RESUME_JOB_ROLE,
            op(|s, c| async move { jobs::resume_job(&s, &c, P, 42).await }),
        ),
        (
            "continue_job",
            Role::Operator,
            jobs::CONTINUE_JOB_ROLE,
            op(|s, c| async move { jobs::continue_job(&s, &c, P, 42).await }),
        ),
        (
            "current_transfer",
            Role::Viewer,
            transfer::CURRENT_TRANSFER_ROLE,
            op(|s, c| async move { transfer::current_transfer(&s, &c, P).await }),
        ),
        (
            "cancel_transfer",
            Role::Operator,
            transfer::CANCEL_TRANSFER_ROLE,
            op(|s, c| async move { transfer::cancel_transfer(&s, &c, P, 7).await }),
        ),
        (
            "file_info",
            Role::Viewer,
            files::FILE_INFO_ROLE,
            op(|s, c| async move { files::file_info(&s, &c, P, "usb", Some(F)).await }),
        ),
        (
            "file_state",
            Role::Viewer,
            files::FILE_STATE_ROLE,
            op(|s, c| async move { files::file_state(&s, &c, P, "usb", F).await }),
        ),
        (
            "print_file",
            Role::Operator,
            files::PRINT_FILE_ROLE,
            op(|s, c| async move { files::print_file(&s, &c, P, "usb", F).await }),
        ),
        (
            "delete_file",
            Role::Admin,
            files::DELETE_FILE_ROLE,
            op(|s, c| async move {
                files::delete_file(&s, &c, P, "usb", F, DeleteOptions::default()).await
            }),
        ),
        (
            "upload_file",
            Role::Operator,
            files::UPLOAD_FILE_ROLE,
            op(|s, c| async move {
                files::upload_file(
                    &s,
                    &c,
                    P,
                    "usb",
                    "a.bgcode",
                    UploadBody::from_bytes(vec![1]),
                    UploadOptions::default(),
                )
                .await
            }),
        ),
        (
            "list_cameras",
            Role::Viewer,
            cameras::LIST_CAMERAS_ROLE,
            op(|s, c| async move { cameras::list_cameras(&s, &c, P).await }),
        ),
        (
            "camera_detail",
            Role::Viewer,
            cameras::CAMERA_DETAIL_ROLE,
            op(|s, c| async move { cameras::camera_detail(&s, &c, P, "cam1").await }),
        ),
        (
            "snapshot",
            Role::Viewer,
            cameras::SNAPSHOT_ROLE,
            op(|s, c| async move { cameras::snapshot(&s, &c, P, None).await }),
        ),
        (
            "order_cameras",
            Role::Admin,
            cameras::ORDER_CAMERAS_ROLE,
            op(|s, c| async move { cameras::order_cameras(&s, &c, P, vec!["cam1".into()]).await }),
        ),
        (
            "register_camera",
            Role::Admin,
            cameras::REGISTER_CAMERA_ROLE,
            op(|s, c| async move {
                cameras::register_camera(&s, &c, P, "cam1", CameraSettings::default()).await
            }),
        ),
        (
            "remove_camera",
            Role::Admin,
            cameras::REMOVE_CAMERA_ROLE,
            op(|s, c| async move { cameras::remove_camera(&s, &c, P, "cam1").await }),
        ),
        (
            "update_camera_config",
            Role::Admin,
            cameras::UPDATE_CAMERA_CONFIG_ROLE,
            op(|s, c| async move {
                cameras::update_camera_config(&s, &c, P, "cam1", CameraSettings::default()).await
            }),
        ),
        (
            "reset_camera_config",
            Role::Admin,
            cameras::RESET_CAMERA_CONFIG_ROLE,
            op(|s, c| async move { cameras::reset_camera_config(&s, &c, P, "cam1").await }),
        ),
        (
            "connect_camera",
            Role::Admin,
            cameras::CONNECT_CAMERA_ROLE,
            op(|s, c| async move { cameras::connect_camera(&s, &c, P, "cam1").await }),
        ),
        (
            "disconnect_camera",
            Role::Admin,
            cameras::DISCONNECT_CAMERA_ROLE,
            op(|s, c| async move { cameras::disconnect_camera(&s, &c, P, "cam1").await }),
        ),
        (
            "take_snapshot",
            Role::Operator,
            cameras::TAKE_SNAPSHOT_ROLE,
            op(|s, c| async move { cameras::take_snapshot(&s, &c, P, "cam1").await }),
        ),
        (
            "check_update",
            Role::Admin,
            update::CHECK_UPDATE_ROLE,
            op(|s, c| async move { update::check_update(&s, &c, P, "prusalink").await }),
        ),
        (
            "apply_update",
            Role::Admin,
            update::APPLY_UPDATE_ROLE,
            op(|s, c| async move { update::apply_update(&s, &c, P, "prusalink").await }),
        ),
    ]
}

#[tokio::test]
async fn operations_enforce_roles() {
    let probe = TestApi::start().await;
    let count = operations(probe.fake.addr.to_string()).len();
    assert_eq!(count, 32, "one entry per printers:: operation");

    for index in 0..count {
        // A fresh printer per operation, since some of them remove or add one.
        let api = TestApi::start().await;
        let (name, spec, constant, call) = operations(api.fake.addr.to_string()).remove(index);
        assert_eq!(
            constant, spec,
            "{name}: role const differs from the spec table"
        );

        if spec > Role::Viewer {
            let result = call(api.state.clone(), caller(one_below(spec))).await;
            assert!(
                matches!(result, Err(ApiError::Forbidden(_))),
                "{name} accepted a caller below {spec}"
            );
            assert!(
                api.fake.requests().is_empty(),
                "{name} contacted the printer before checking the role"
            );
        }

        let result = call(api.state.clone(), caller(spec)).await;
        assert!(
            !matches!(result, Err(ApiError::Forbidden(_))),
            "{name} refused a caller with exactly {spec}"
        );
    }
}

/// Every registered route, once, with the minimum role it needs.
fn routes() -> Vec<(Method, &'static str, Role)> {
    use Method as M;
    use Role::{Admin, Operator, Viewer};
    vec![
        // Viewer
        (M::GET, "/api/printers", Viewer),
        (M::GET, "/api/printers/mk4", Viewer),
        (M::GET, "/api/printers/mk4/info", Viewer),
        (M::GET, "/api/printers/mk4/status", Viewer),
        (M::GET, "/api/printers/mk4/storage", Viewer),
        (M::GET, "/api/printers/mk4/job", Viewer),
        (M::GET, "/api/printers/mk4/transfer", Viewer),
        (M::GET, "/api/printers/mk4/files/usb", Viewer),
        (
            M::GET,
            "/api/printers/mk4/files/usb/cube-12mm.bgcode",
            Viewer,
        ),
        (
            M::HEAD,
            "/api/printers/mk4/files/usb/cube-12mm.bgcode",
            Viewer,
        ),
        (M::GET, "/api/printers/mk4/cameras", Viewer),
        (M::GET, "/api/printers/mk4/cameras/snap", Viewer),
        (M::GET, "/api/printers/mk4/cameras/cam1", Viewer),
        (M::GET, "/api/printers/mk4/cameras/cam1/snap", Viewer),
        // Operator
        (M::DELETE, "/api/printers/mk4/job/42", Operator),
        (M::PUT, "/api/printers/mk4/job/42/pause", Operator),
        (M::PUT, "/api/printers/mk4/job/42/resume", Operator),
        (M::PUT, "/api/printers/mk4/job/42/continue", Operator),
        (M::DELETE, "/api/printers/mk4/transfer/7", Operator),
        (
            M::PUT,
            "/api/printers/mk4/files/usb/cube-12mm.bgcode",
            Operator,
        ),
        (
            M::POST,
            "/api/printers/mk4/files/usb/cube-12mm.bgcode/print",
            Operator,
        ),
        (M::POST, "/api/printers/mk4/cameras/cam1/snap", Operator),
        // Admin
        (M::POST, "/api/printers", Admin),
        (M::DELETE, "/api/printers/mk4", Admin),
        (
            M::DELETE,
            "/api/printers/mk4/files/usb/cube-12mm.bgcode",
            Admin,
        ),
        (M::PUT, "/api/printers/mk4/cameras/order", Admin),
        (M::POST, "/api/printers/mk4/cameras/cam1", Admin),
        (M::DELETE, "/api/printers/mk4/cameras/cam1", Admin),
        (M::PATCH, "/api/printers/mk4/cameras/cam1/config", Admin),
        (M::DELETE, "/api/printers/mk4/cameras/cam1/config", Admin),
        (M::POST, "/api/printers/mk4/cameras/cam1/connection", Admin),
        (
            M::DELETE,
            "/api/printers/mk4/cameras/cam1/connection",
            Admin,
        ),
        (M::GET, "/api/printers/mk4/update/prusalink", Admin),
        (M::POST, "/api/printers/mk4/update/prusalink", Admin),
    ]
}

/// The PrusaLink call each route makes (method, uri), keyed by the route.
/// The four registry routes talk to no printer, so they have no entry.
fn upstream(method: &Method, uri: &str) -> Option<(Method, &'static str)> {
    let files = "/api/v1/files/usb/cube-12mm.bgcode";
    let up = match (method.as_str(), uri.trim_start_matches("/api/printers/mk4")) {
        ("GET", "/info") => (Method::GET, "/api/v1/info"),
        ("GET", "/status") => (Method::GET, "/api/v1/status"),
        ("GET", "/storage") => (Method::GET, "/api/v1/storage"),
        ("GET", "/job") => (Method::GET, "/api/v1/job"),
        ("GET", "/transfer") => (Method::GET, "/api/v1/transfer"),
        ("GET", "/files/usb") => (Method::GET, "/api/v1/files/usb/"),
        ("GET", "/files/usb/cube-12mm.bgcode") => (Method::GET, files),
        ("HEAD", "/files/usb/cube-12mm.bgcode") => (Method::HEAD, files),
        ("GET", "/cameras") => (Method::GET, "/api/v1/cameras"),
        ("GET", "/cameras/snap") => (Method::GET, "/api/v1/cameras/snap"),
        ("GET", "/cameras/cam1") => (Method::GET, "/api/v1/cameras/cam1"),
        ("GET", "/cameras/cam1/snap") => (Method::GET, "/api/v1/cameras/cam1/snap"),
        ("DELETE", "/job/42") => (Method::DELETE, "/api/v1/job/42"),
        ("PUT", "/job/42/pause") => (Method::PUT, "/api/v1/job/42/pause"),
        ("PUT", "/job/42/resume") => (Method::PUT, "/api/v1/job/42/resume"),
        ("PUT", "/job/42/continue") => (Method::PUT, "/api/v1/job/42/continue"),
        ("DELETE", "/transfer/7") => (Method::DELETE, "/api/v1/transfer/7"),
        ("PUT", "/files/usb/cube-12mm.bgcode") => (Method::PUT, files),
        ("POST", "/files/usb/cube-12mm.bgcode/print") => (Method::POST, files),
        ("POST", "/cameras/cam1/snap") => (Method::POST, "/api/v1/cameras/cam1/snap"),
        ("DELETE", "/files/usb/cube-12mm.bgcode") => (Method::DELETE, files),
        ("PUT", "/cameras/order") => (Method::PUT, "/api/v1/cameras"),
        ("POST", "/cameras/cam1") => (Method::POST, "/api/v1/cameras/cam1"),
        ("DELETE", "/cameras/cam1") => (Method::DELETE, "/api/v1/cameras/cam1"),
        ("PATCH", "/cameras/cam1/config") => (Method::PATCH, "/api/v1/cameras/cam1/config"),
        ("DELETE", "/cameras/cam1/config") => (Method::DELETE, "/api/v1/cameras/cam1/config"),
        ("POST", "/cameras/cam1/connection") => (Method::POST, "/api/v1/cameras/cam1/connection"),
        ("DELETE", "/cameras/cam1/connection") => {
            (Method::DELETE, "/api/v1/cameras/cam1/connection")
        }
        ("GET", "/update/prusalink") => (Method::GET, "/api/v1/update/prusalink"),
        ("POST", "/update/prusalink") => (Method::POST, "/api/v1/update/prusalink"),
        _ => return None,
    };
    Some(up)
}

/// Sends one table row, as `role` (or anonymously when `None`).
async fn send_row(api: &TestApi, role: Option<Role>, method: &Method, uri: &str) -> TestResponse {
    let body: Option<String> = match (method.as_str(), uri) {
        ("POST", "/api/printers") => Some(
            json!({"name":"newp","kind":"mk4","host":api.fake.addr.to_string(),
                   "username":USERNAME,"password":PASSWORD})
            .to_string(),
        ),
        ("PUT", u) if u.ends_with("/cameras/order") => Some("[]".to_owned()),
        ("POST" | "PUT" | "PATCH", _) => Some("{}".to_owned()),
        _ => None,
    };
    let length = body.as_ref().map(|b| b.len().to_string());
    let mut headers: Vec<(&str, &str)> = Vec::new();
    if let Some(length) = &length {
        headers.push(("content-type", "application/json"));
        headers.push(("content-length", length));
    }
    let body = Body::from(body.unwrap_or_default());
    match role {
        Some(role) => api.call(role, method.clone(), uri, &headers, body).await,
        None => {
            api.call_anonymous(method.clone(), uri, &headers, body)
                .await
        }
    }
}

/// A bare 404/405 from the router (as opposed to the API's own JSON errors).
fn is_routing_failure(r: &TestResponse) -> bool {
    r.status == StatusCode::METHOD_NOT_ALLOWED
        || (r.status == StatusCode::NOT_FOUND && r.body.is_empty())
}

#[tokio::test]
async fn role_table_has_every_route() {
    let routes = routes();
    assert_eq!(routes.len(), 34);
    for (method, uri, _) in &routes {
        let api = TestApi::start().await;
        let r = send_row(&api, Some(Role::Admin), method, uri).await;
        assert!(
            !is_routing_failure(&r),
            "{method} {uri} is not routed: {} {}",
            r.status,
            r.text()
        );
    }

    let api = TestApi::start().await;
    for (method, uri) in [
        (Method::GET, "/api/printers/mk4/cameras/order"),
        (Method::POST, "/api/printers/mk4/job"),
    ] {
        let r = send_row(&api, Some(Role::Admin), &method, uri).await;
        assert!(
            matches!(
                r.status,
                StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
            ),
            "{method} {uri} should not be routed, got {}",
            r.status
        );
    }
}

#[tokio::test]
async fn every_route_maps_upstream_errors() {
    let mut covered = 0;
    for (method, uri, _) in routes() {
        let Some((up_method, up_uri)) = upstream(&method, uri) else {
            continue;
        };
        covered += 1;

        // 409 → 409 for every route, with the printer's own words passed on.
        let api = TestApi::start().await;
        api.fake.set(
            up_method.clone(),
            up_uri,
            Canned::json(409, json!({"title":"Conflict","text":"zz-conflict-text"})),
        );
        let r = send_row(&api, Some(Role::Admin), &method, uri).await;
        assert_eq!(
            r.status,
            StatusCode::CONFLICT,
            "{method} {uri}: {}",
            r.text()
        );
        if method != Method::HEAD {
            assert!(
                r.text().contains("zz-conflict-text"),
                "{method} {uri}: {}",
                r.text()
            );
        }

        // 503 → 502 for the reads.
        if method == Method::GET {
            let api = TestApi::start().await;
            api.fake.set(up_method, up_uri, Canned::empty(503));
            let r = send_row(&api, Some(Role::Admin), &method, uri).await;
            assert_eq!(
                r.status,
                StatusCode::BAD_GATEWAY,
                "{method} {uri}: {}",
                r.text()
            );
            assert!(
                r.text().contains("unavailable"),
                "{method} {uri}: {}",
                r.text()
            );
        }
    }
    assert_eq!(covered, 30, "every route except the four registry ones");
}

#[tokio::test]
#[ignore = "enabled when auth lands"]
async fn every_route_requires_credentials() {
    let api = TestApi::start().await;
    for (method, uri, _) in routes() {
        let r = send_row(&api, None, &method, uri).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}

#[tokio::test]
#[ignore = "enabled when auth lands"]
async fn every_route_rejects_the_role_below_its_minimum() {
    let api = TestApi::start().await;
    for (method, uri, min) in routes() {
        if min == Role::Viewer {
            continue;
        }
        let r = send_row(&api, Some(one_below(min)), &method, uri).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{method} {uri}");
    }
}
