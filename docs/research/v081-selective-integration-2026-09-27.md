# v0.8.1 선별 소스 통합 — 2026-09-27

## 범위와 권위

- 통합 브랜치: `codex/v081-selective-source-ci-20260927`.
- 제품 기준선: 원격 main
  `7d7fc390b2f65d52ebf8362dff65dcbfcb6d481b`.
- 원본 구현: `codex/converge-v081-v090-release-20260923`의
  `170026643db28b82b33a12bece5496e65c3bb947` 및 당시 미커밋 변경.
- 원본의 혼합 v0.8.1/v0.9.0 이력과 작업트리는 보존한다. 전체 브랜치 병합 대신
  v0.8.1 source owner별 변경을 적용하고 최신 main과 겹치는 경계를 직접 대조했다.
- PC4 TB 계산·I/O·병렬화 업그레이드는 동결한다. 기존 제품의 TB provider,
  transport, parser, graph generation과 기존 routing은 변경하지 않는다.
- 이 문서는 새 통합 소스의 좁은 로컬 검증이다. 자산의 기존 qualification,
  앱 qualification, canonical acceptance, 성능 채택, 배포 권위를 서로 바꾸어
  해석하지 않는다. exact source는 해당 commit과 비게시 CI의 `head_sha`로 결박한다.

## 통합한 경계

1. typed original-row codec, exact legal-board, conditioned local relation의 parser,
   signed catalog, immutable owner 및 요청별 독립 스위치.
2. BuildUp의 concrete-edge 이후 negative 조회와 닫힌 Boolean relation의 reverse
   success seed 합성. CountAll, witness, spin, finesse의 기존 exact 경로는 유지한다.
3. 요청별 immutable StandardBag supply 공유, 최초 aggregate/coverage owner 이동,
   private/shared/memo exit accounting. 상태-major memo는 local-only A/B 후보다.
4. CLI의 명시 legal-board/reachability-pack 관리, 동일 signed generation 손상 복구,
   Web의 취소 가능한 다운로드와 OPFS, Desktop의 RAII lease 및 native store 연결,
   Discord immutable compute image의 명시 자산 provisioning.
5. 실제 worker 수와 memory scope가 맞지 않는 표본의 거부, source/binary identity 및
   압력·censored 상태를 보존하는 별도 benchmark harness.

다섯 legal-board와 다섯 conditioned pack은 기존 immutable Release 자산을 그대로
참조한다. 바이너리를 Git source에 복사하거나 새로 생성하지 않았고 qualification을
반복하지 않았다. `clearra-pc4-qualifier`는 v0.8.1 생성·검증·서명 명령만 포함한다.
기존 TB의 field-hash codec 사용은 이 도구의 domain 표현에 한정한다. Core의 기존
edge materializer는 conditioned product를 끈 독립 검증 경로를 유지한다.

## 최신 main에서 보존한 계약

- optional boundary recovery, mandatory/pinned와 실제 Build target drawing.
- rule/spin profile selector 및 command registry.
- native verified host와 실행 정책/worker admission, 기존 PC4 curl/online feature.
- WASM reset의 borrowed-runtime 거절과 fail-operational 오류 경계.
- deterministic watchdog heartbeat의 진행/미진행 계약.

워커 계약 테스트의 대기 fixture는 실행 grant를 받기 전에 멈추는 방식에서 grant 이후
consumer를 멈추는 방식으로 조정했다. 이에 따라 실제 실행 중인 worker만 세는 새
progress 계약과 최신 watchdog 테스트를 동시에 보존한다. production retry/watchdog
정책을 테스트 편의를 위해 완화하지 않는다.

GUI legacy Setup builder가 검증만 하던 두 accelerator 스위치는 실제 typed query에
전달하도록 수정했다. 네 가지 on/off 조합의 제품 계약 테스트를 추가했다. App/CLI의
일반 execution failure는 더 이상 unsupported로 일괄 분류하지 않지만 명시적인
runtime refusal은 기존 코드를 그대로 보존한다.

## 새 통합 소스의 로컬 검증

| 검증 | 결과 | 한계 |
| --- | --- | --- |
| Core 기본 경로 `cargo check --locked --offline` | 통과, 경고 없음 | 제품 executable 실행 아님 |
| Core `parallel,local-search-ab,qualification-reference` | 통과, 경고 없음 | 새로운 memo/performance A/B 아님 |
| CLI/GUI host/WASM ABI 및 v0.8.1 generator의 native tests typecheck | 소스 컴파일 통과 | wrapper 종료 시 descendant 종료 경고로 감독 실행 전체를 성공 처리하지 않음 |
| CLI `wasm-cpu-runtime,local-search-ab` typecheck | 통과 | 새 native binary의 실제 실행 아님 |
| 변경 Rust package의 fmt, staged/unstaged diff check | 통과 | 릴리스 전체 lint/acceptance 아님 |
| harness·memory·memo 및 Web/Desktop transfer 순수 Node | 31 passed | solver·브라우저·Tauri readback 아님 |
| frontend staging·accepted Pages build·Discord immutable asset mock | 26 passed | Release asset 다운로드/이미지 배포 아님 |
| 공유 UI 전체 표준 test task | Node 320 passed, TypeScript 계약 14개 및 typecheck 통과 | serialization 15개는 이 결과에 포함; 실제 UI smoke 아님 |
| 공유 UI/Web worker typecheck | 통과 | Svelte/브라우저의 전체 surface 실행 아님 |
| Web worker TypeScript contracts | 16개 통과 | 실제 WASM/브라우저 탐색과 peak 아님 |
| 새 Rust unit/integration 실행 및 WASM target check | Linux 비게시 CI 대기 | 과거 Windows 실행 정책 차단을 우회하거나 WSL에서 재시도하지 않음 |

native 제품 typecheck의 감독 영수증은 `tree-not-stopped`, return 125를 기록했다.
최종 `process_tree_stopped=true`, 압력 0, aggregate commit peak 1,181,294,592바이트다.
소스 컴파일 성공과 감독 clean-exit의 실패를 분리하며 이를 제품 peak 또는 OOM으로
해석하지 않는다. 후속 Core 기본/병렬 typecheck와 CLI A/B typecheck는 별도의 정상
감독 종료다. 원시 영수증/로그는 기존 선언된 로컬 artifact/state root에만 둔다.

## 비게시 CI와 다음 gate

`.github/workflows/v081-selective-source-ci.yml`은 이 브랜치에만 반응하고 `contents:
read`로 Core, native products, WASM ABI, surface contracts를 분리 실행한다. 로컬 전용
selector는 별도 feature 검사에서만 사용한다. 같은 job의 test feature를 통일해 작은
필터별 실행에서도 컴파일 산출물을 재사용한다. 새로운 generation의 전수 생성,
Release asset 게시, tag 생성, main promote, Pages/Discord/Cloud 배포는 수행하지 않는다.

남은 항목은 다음 순서로 진행한다.

1. 이 exact source의 focused Rust/WASM CI 결과와 실패 경계를 확인한다.
2. 실행 가능한 새 native binary로 state-major/reference의 11-worker ABBA를 먼저
   수행한다. 새 selector/accounting이 없는 이전 binary 표본은 하네스가 거절한다.
3. legal-board와 conditioned relation을 같은 binary로 각각 분리 ABBA하고 결합 효과는
   그 이후 평가한다. clean timing과 profiling을 구분하며 실제 worker, digest,
   OS peak, private/shared bytes 및 압력/censor를 함께 확인한다.
4. 작은 입력/1~6L/초기 필드/CountAll/minimum/replay와 1·2·11-worker parity 및 실제
   CLI/Web/Desktop/Discord surface readback을 완료한다.
5. BuildUp p95 20%, 전체 p95 10%, 작은 입력 회귀 5% 이하와 active asset shared peak
   128MiB gate를 증명한 후에만 main exact-SHA acceptance/배포/rollback으로 진입한다.

현재 **v0.8.1 앱은 No-Go**다. 과거 executable의 8개 ABBA 표본은 결과 동일성과 호출
활성화만 보였고 압력 및 성능 이득 미입증으로 성능 gate를 닫지 못했다. 그 영수증은
[`v081-accelerator-worker-product-closure-2026-09-27.md`](v081-accelerator-worker-product-closure-2026-09-27.md)에
보존한다. 이 선별 통합의 benchmark 또는 qualification으로 재사용하지 않는다.

## 첫 비게시 CI 확인과 경로 수정

`a2ddc27d82d3a83d820bf7537305097245d2efb0`의
[36301789552](https://github.com/daejunnom/Clearra/actions/runs/36301789552)는 failure다.

| 작업 | 관측 결과 | 권위 |
| --- | --- | --- |
| Core | 기본/parallel/WASM target check 및 5개 집중 필터 통과 | 새 소스의 v081/memo, reachability, row codec, legal-board, conditioned relation 집중 증거 |
| WASM ABI | WASM target check, reset 1개, accelerator 2개 테스트 통과 | 실제 WASM 빌드·브라우저 탐색 증거 아님 |
| Surfaces | 모든 단계 통과 | 순수 계약·mock·typecheck이며 실제 제품 readback 아님 |
| Native products | 제품 타입 검사와 정책/서명/런타임 집중 테스트 통과; repair 7개 중 6개 실패 | 자료 lifecycle 전체 완료 아님 |
| Bounded generator proofs | 앞 단계 실패로 미실행 | Open |

6개 repair 실패는 실제 파일 작업 이전의 fixture root assertion이다. CI가
`v081-selective-store-tests`를 전달했지만 Rust fixture의 폐쇄 허용 경로는
`accelerator-store-tests`다. CI만 기존 경로에 맞추고 컴파일 전 동일 경로 검사를
추가했다. Rust의 traversal/link/repository 정책이나 삭제 범위를 완화하지 않는다.
이 결박과 조기 거절 계약은 새 순수 Node 테스트 두 개로 확인한다.

native test family 하나가 실패하면 뒤의 독립된 CLI family와 bounded proof도
피드백을 남기도록 했다. 실패 자체는 nonzero 종료로 유지하고 `continue-on-error`는
사용하지 않는다. typecheck 실패 또는 취소 상태에서는 뒤의 실행을 허용하지 않는다.

CI에서 나온 WASM complete-candidate constructor 2개 및 App의 bitmap-off encoding
2개 dead-code 경고는 수정하지 않은 main 소스의 feature/target 조합이다. 검색 의미를
바꾸거나 전역 lint suppression을 추가하지 않았으며 별도 정리 대상으로 기록한다.
