use std::sync::Arc;

use crate::params::XrossGuitarAmpParams;

pub struct GainProcessor {
    pub params: Arc<XrossGuitarAmpParams>,
}

impl GainProcessor {
    pub fn new(params: Arc<XrossGuitarAmpParams>) -> Self {
        Self { params }
    }
}
