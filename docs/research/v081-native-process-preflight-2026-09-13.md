# v0.8.1 native process verification preflight

No native process test passed in this follow-up. Source baseline was `88ce123`.

1. A managed Cargo invocation selecting `native-c-core,wasm-cpu-runtime` and
   the `process_e2e` test target failed compiling the FFI crate because the
   required `clearra_core` static library had not been prepared. No process
   test executable ran. This was an incomplete build invocation, not a product
   assertion failure.
2. Added `scripts/tools/check-native-cli-contracts.ps1` to compose the existing
   ProductE2E native-library builder, native archive content/link identity
   helper, and exact CLI process test target in one managed build transaction.
   It first checks canonical Cargo output and the existing Trusted/application
   control policy. It is explicitly Windows/MSVC-only.
3. Invoking it through the managed owner with `-ExecutionSurface Trusted`
   returned `E_WINDOWS_GENERATED_EXECUTION_REQUIRES_APPROVED_PACKAGE`: the
   preflight detected enforced Windows UMCI. It stopped before building the C
   archive or invoking Cargo. The command terminated with exit 1.

No application-control changes, signing/unblocking attempts, alternate-runtime
substitution, deployment, or retry past that preflight were performed. The
helper's successful native build/test path remains unverified on this machine;
its observed result is the intended preflight refusal. The native process gate
must be run in an allowed environment or against appropriately approved release
artifacts using the existing product execution entrypoints. Previous local
wasm-cpu-runtime CLI tests remain separately scoped evidence, not proof that the
native process gate has passed.

The broader v0.8.1/v0.9.0 work is not at an impasse: source/static checks and
online PC4 implementation remain possible. All commands from this follow-up
are terminal; no test or deployment is being polled.
