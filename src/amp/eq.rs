use std::sync::Arc;

use crate::params::XrossGuitarAmpParams;

pub struct EqProcessor {
    pub params: Arc<XrossGuitarAmpParams>,
}

impl EqProcessor {
    pub fn new(params: Arc<XrossGuitarAmpParams>) -> Self {
        Self { params }
    }
}
