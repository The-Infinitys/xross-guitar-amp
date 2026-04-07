use std::sync::Arc;

use crate::params::XrossGuitarAmpParams;

pub struct CabProcessor {
    params: Arc<XrossGuitarAmpParams>,
}

impl CabProcessor {
    pub fn new(params: Arc<XrossGuitarAmpParams>) -> Self {
        Self { params }
    }
}
