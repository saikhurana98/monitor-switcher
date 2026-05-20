#![cfg_attr(
    test,
    allow(
        clippy::panic,
        clippy::redundant_clone,
        clippy::missing_const_for_fn,
        clippy::option_if_let_else
    )
)]

pub mod config;
pub mod ddc;
pub mod state;
pub mod switcher;
pub mod tray;
