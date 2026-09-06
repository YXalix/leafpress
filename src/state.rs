use std::sync::Arc;

use axum::extract::FromRef;
use leptos::prelude::LeptosOptions;

use crate::config::Config;
use crate::content::SharedIndex;

#[derive(Clone, FromRef)]
pub struct AppState {
    pub leptos_options: LeptosOptions,
    pub pool: sqlx::SqlitePool,
    pub index: SharedIndex,
    pub config: Arc<Config>,
}
