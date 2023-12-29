pub mod global_fns;
pub mod tera;

use libs::once_cell::sync::Lazy;
use libs::tera::Tera;

pub use global_fns::Hera;

pub static BLN_TERA: Lazy<Tera> = Lazy::new(|| {
    let tera = Tera::default();

    tera
});
