use crate::AppResponse;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CliCompatResponse {
    response: AppResponse,
}

impl CliCompatResponse {
    pub fn new(response: AppResponse) -> Self {
        Self { response }
    }
}
impl CliCompatResponse {
    pub fn response(&self) -> &AppResponse {
        &self.response
    }
}
