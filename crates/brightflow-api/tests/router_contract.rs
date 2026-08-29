//! Route-layer contracts, tested against the router as production mounts it.
//!
//! The unit tests on `shared::path_guard` and `routes::require_auth` prove
//! the predicates; these tests prove the *wiring* — that the layers are
//! actually attached to the router `serve` runs. That distinction is the
//! whole value: a predicate that passes its unit tests but is mounted on the
//! wrong sub-router would pass everything except this file.
//!
//! These are storage-backed and boot from a copy of the committed test
//! workspace (via `brightflow_test_support`), so the router mounts against a
//! genuinely migrated Litehouse/auth pair rather than a bare temp dir — the
//! same data-less scaffolding production would boot, but with a real schema
//! and committed fixture to resolve through.

#![expect(
    clippy::unwrap_used,
    reason = "integration tests panic on failure by design"
)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use brightflow_api::auth::AuthDb;
use brightflow_api::state::AppState;
use brightflow_store::ParquetStore;
use brightflow_test_support::{copy_template, TestWorkspace};

/// The served app over a copy of the committed test workspace, with permissive
/// CORS (CORS policy is env-dependent and not under test here).
async fn app(ws: &TestWorkspace) -> axum::Router {
    let store = ParquetStore::new(ws.paths.store(), &ws.paths.litehouse_url())
        .await
        .unwrap();
    let state = AppState::with_store(store).await;

    let auth_db = AuthDb::new(&ws.paths.auth_url()).await.unwrap();

    let (app, _sweeper) =
        brightflow_api::build_app(state, auth_db, tower_http::cors::CorsLayer::new());
    app
}

#[tokio::test]
async fn protected_routes_reject_unauthenticated_requests() {
    let ws = copy_template().unwrap();
    let app = app(&ws).await;

    for uri in [
        "/api/sources",
        "/api/connectors/unified",
        "/api/connectors/runs",
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{uri} must sit behind the auth wall"
        );
    }
}

#[tokio::test]
async fn public_routes_do_not_require_a_session() {
    let ws = copy_template().unwrap();
    let app = app(&ws).await;

    // /api/auth/me is public by design (the frontend probes it to decide
    // whether to show the login screen): it must answer without a session,
    // just not with 401-by-middleware.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/auth/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(response.status(), StatusCode::NOT_FOUND);

    let script_response = app
        .oneshot(
            Request::builder()
                .uri("/api/script.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(script_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn path_traversal_in_a_route_param_is_rejected_as_mounted() {
    let ws = copy_template().unwrap();

    // Seed a user directly, then log in through the mounted route: on
    // protected routes the auth wall runs before the path guard, so an
    // unauthenticated traversal probe would only ever see 401 and prove
    // nothing about the guard. (The committed template's own demo user is a
    // separate account; create_user adds this one alongside it.)
    let auth_db = AuthDb::new(&ws.paths.auth_url()).await.unwrap();
    let hash = brightflow_api::auth::hash_password("hunter2!").unwrap();
    auth_db
        .create_user("t@example.com", "T", &hash)
        .await
        .unwrap();

    let store = ParquetStore::new(ws.paths.store(), &ws.paths.litehouse_url())
        .await
        .unwrap();
    let state = AppState::with_store(store).await;
    let (app, _sweeper) =
        brightflow_api::build_app(state, auth_db, tower_http::cors::CorsLayer::new());

    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"email":"t@example.com","password":"hunter2!"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK, "seeded login must succeed");
    let cookie = login
        .headers()
        .get("set-cookie")
        .expect("login sets a session cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    // An encoded `../` inside a dynamic segment must be stopped by the
    // route-level guard: 400 from the guard, never a handler response built
    // from the hostile segment.
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/sources/..%2F..%2F..%2Ftmp/tables/x/insights/history")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "traversal in a path param must be rejected by the guard layer"
    );
}
