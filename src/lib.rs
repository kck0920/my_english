pub mod ai;
pub mod auth;
pub mod cards;
pub mod db;
pub mod domain;
pub mod import_export;
pub mod review;
pub mod state;
pub mod tts;
pub mod ui;

use axum::{middleware, Router};
use state::AppState;

pub fn build_app(state: AppState) -> Router {
    let protected = Router::new()
        .merge(cards::routes())
        .merge(ai::routes())
        .merge(review::routes())
        .merge(tts::routes())
        .merge(import_export::routes())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    Router::new()
        .merge(auth::public_routes())
        .merge(tts::public_routes())
        .merge(protected)
        .with_state(state)
}
