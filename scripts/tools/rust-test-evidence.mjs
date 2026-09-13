// SRP: observe libtest execution evidence without another Cargo invocation or
// retaining its full output. A successful process with zero tests is not proof.
export function createRustTestEvidence() {
  let tail = '';
  let executed = false;
  return {
    observe(chunk) {
      const text = tail + chunk.toString();
      executed ||= /(?:^|\n)test result: ok\. [1-9][0-9]* passed;/u.test(text);
      tail = text.slice(-256);
    },
    hasExecutedTests: () => executed,
  };
}
