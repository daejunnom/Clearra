use crate::{AppContext, AppRequest, AppResponse};

pub fn run_request(context: &AppContext, request: AppRequest) -> AppResponse {
    context.run(request)
}
