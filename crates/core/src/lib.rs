//! ethshowcase-core: scrape, store, and serve the ETHGlobal Showcase dataset.
//!
//! Pipeline:
//! 1. [`Scraper::scrape_index`] walks showcase listing pages (~440 requests)
//!    and extracts full project objects embedded in the RSC flight payload.
//! 2. [`Scraper::scrape_details`] fetches each project's detail page for the
//!    long-form description / how-it's-made texts.
//! 3. [`store::Db`] persists everything to SQLite with FTS5 search; [`site`]
//!    generates a static search site; [`serve`] exposes a queryable API.

pub mod cache;
pub mod error;
pub mod flight;
pub mod http;
pub mod model;
pub mod parse;
pub mod scrape;
pub mod serve;
pub mod site;
pub mod store;

pub use error::{Error, Result};
pub use model::{Event, Project, Stats};
pub use scrape::{BASE_URL, IndexOptions, Scraper, default_scraper};
pub use store::{Db, ProjectQuery};
