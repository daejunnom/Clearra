'use strict';

// Loaded only for a directly supervised Node root. Descendant Node processes
// inherit the request paths, but only the root PID named in the request may
// acknowledge that its own heap completed a full collection.
const fs = require('node:fs');
const path = require('node:path');

const requestPath = process.env.CLEARRA_RUNTIME_GC_REQUEST_PATH;
const acknowledgementPath = process.env.CLEARRA_RUNTIME_GC_ACK_PATH;
const protocol = process.env.CLEARRA_RUNTIME_GC_PROTOCOL;
if (requestPath && acknowledgementPath && protocol && typeof global.gc === 'function') {
  let lastRequestId;
  const respond = () => {
    let request;
    try {
      request = JSON.parse(fs.readFileSync(requestPath, 'utf8'));
    } catch {
      return;
    }
    if (request.schema_id !== protocol || request.action !== 'full-gc' ||
        request.child_pid !== process.pid || typeof request.request_id !== 'string' ||
        request.request_id.length === 0 || request.request_id === lastRequestId) {
      return;
    }
    const temporary = `${acknowledgementPath}.${process.pid}.tmp`;
    try {
      global.gc();
      const acknowledgement = {
        schema_id: protocol,
        request_id: request.request_id,
        action: 'full-gc',
        status: 'completed',
        child_pid: process.pid,
      };
      fs.writeFileSync(temporary, JSON.stringify(acknowledgement));
      fs.renameSync(temporary, acknowledgementPath);
      lastRequestId = request.request_id;
    } catch {
      // The supervisor treats a missing acknowledgement as an unconfirmed GC.
      try { fs.unlinkSync(temporary); } catch { /* No staged acknowledgement. */ }
    }
  };
  try {
    const watcher = fs.watch(path.dirname(requestPath), respond);
    watcher.unref();
    respond();
  } catch {
    // The root process is allowed to continue under the hard memory boundary.
  }
}
