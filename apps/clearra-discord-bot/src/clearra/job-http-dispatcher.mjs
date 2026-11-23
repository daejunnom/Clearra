import { getGlobalDispatcher } from "undici";

// A synchronous job sends headers after computation. Preserve the current
// proxy/pool/body limits while removing only the default header-wait deadline.
export function createJobResponseDispatcher(dispatcher = null) {
  return Object.freeze({
    dispatch(options, handler) {
      return (dispatcher ?? getGlobalDispatcher()).dispatch(
        { ...options, headersTimeout: 0 },
        handler,
      );
    },
  });
}

export const jobResponseDispatcher = createJobResponseDispatcher();
