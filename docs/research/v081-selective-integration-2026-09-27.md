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

## 비게시 CI 통과와 Windows catalog 체크아웃 결함

`896f3955801b4aba9e5a6cae3a8166d1b19d9daa`의
[36303579824](https://github.com/daejunnom/Clearra/actions/runs/36303579824)는
Core, native products, WASM ABI, surface contracts 네 job 모두 success다.
이는 Linux의 집중 소스 검증이며 Windows 실제 제품 readback을 대신하지 않는다.

이 소스의 native 11-worker P7P4 state-major/flat ABBA는 네 표본 모두 complete,
456,923개 family와 full set/candidate identity가 일치했다. 평균 aggregate commit
peak는 state-major가 9.10% 작았지만 A1의 memory pressure와 작은 입력의 directory
overhead 때문에 제품 기본값 변경이나 성능 gate 통과로 처리하지 않는다.
검토 요약과 원시 영수증은 선언된 로컬
`_local/artifacts/v081-state-major-abba-20260927/`에만 보관한다.

이어서 legal-board/conditioned ABBA는 Windows native asset `status` preflight에서
`embedded signed catalog is invalid`로 중단됐다. timed sample, 다운로드, generation
교체는 없었다. Git의 `core.autocrlf=true`가 public keyring 및 두 catalog의 checkout
bytes에 CRLF를 만들었고, `include_str!`가 이를 그대로 포함한 것이 원인이다.
LF 정규형의 catalog digest는 기존 qualified 자산의 identity와 정확히 동일하다.

- legal-board: `2bfce644a9813cfe54fbc88ee376273f60cf1859c7856d25f6f180d4ff52e116`
- conditioned: `5da97080db94441e67209b8295b82dd55041b9c8747b704ce104a4da4dd1fa12`

세 파일에만 명시 `text eol=lf`를 적용하고, 실제 canonical bytes와 Windows autocrlf
설정에서도 LF attribute가 우선함을 검사하는 집중 계약을 추가했다. 서명/검증 규칙,
public key, catalog 내용, asset generation은 변경하지 않는다. 기존 두 CI 계약과
새 두 계약은 로컬에서 4/4 통과했다. 수정된 executable의 native status와 가속기
ABBA는 별도 후속 검증이며, 기존 Linux CI success를 이 수정의 success로 재사용하지 않는다.

## State-major 동적 제품 정책 후보

사용자의 후속 지시에 따라 제품 정책을 `adaptive`로 변경한다. 작은 입력의
디렉터리 비용을 피하기 위해 새 요청/워커의 product memo는 항상 flat으로 시작한다.
피스 수나 요청 워커 수만으로 State-major를 선택하지 않는다.

- 실제 live entry 수가 예상 directory 비용의 두 배를 상쇄할 수 있을 때 첫 cost
  probe를 수행한다. 첫 probe와 이후 probe는 2의 거듭제곱/기하급수적 간격으로 제한한다.
- flat의 전체 `(language, depth, bag, hold) → root`를 정확히 복사한다. row별 실제
  population으로 한 번씩 예약하고, entry capacity payload와 directory를 합친 값이
  flat payload보다 12.5% 이상 작을 때만 기존 owner를 원자적으로 교체한다.
- 이는 logical retained payload의 기준이다. allocator/control bytes, migration 중
  peak, 사용자 wall time의 개선을 증명하는 수치로 표현하지 않는다.
- allocation 또는 key-domain 불일치가 발생하면 이미 삽입된 새 root까지 포함한 flat
  source를 보존하며 그 요청의 promotion을 재시도하지 않는다. resource cap이나 worker
  수를 바꾸지 않는다. 비용만 불리한 probe는 entry 수가 두 배가 될 때 다시 평가한다.
- 큰 요청 안의 epoch recycle은 전환된 capacity를 유지한다. 새로운 작은 요청은 그
  capacity/worker-local node ID를 상속하지 않는다. union memo와 immutable request
  table의 공유/개인 소유 경계는 변경하지 않는다.

영수증에는 `standard_bag_product_memo_policy`, 실제 layout/storage,
promotion attempts 및 promotions를 별도로 남긴다. 다른 크기의 작업을 맡은 워커가
flat/State-major로 나뉘면 실제 layout은 `mixed`, 정책은 `adaptive`로 집계한다.
로컬 A/B의 강제 flat/State-major/compact 제어는 유지하며, 하네스에 `--candidate
adaptive`를 추가했다. 작은 P7 preflight는 flat/zero directory를 요구하고 큰 P7P4
treatment는 실제 promotion 증거를 요구한다. 선택을 무시한 이전 binary는 거절한다.

로컬 증거:

| 검사 | 관측 | 한계 |
| --- | --- | --- |
| 기본 Core 및 parallel typecheck | 경고 없이 통과 | runtime benchmark 아님 |
| `v081_` 집중 Rust | 27/27 통과 | 감독 post-exit 경고는 별도 실패로 보존 |
| A/B feature 없는 제품 정책 테스트 | 5/5 통과, native 명령 exit 0 | 작은 bounded unit test이며 P7P4 whole-product 완주 증거 아님 |
| 하네스/회계/CI 순수 Node 계약 | 27/27 통과 | 실제 새 binary의 ABBA 아님 |
| strict Clippy | 차단: dependency codec 및 `--no-deps` Core의 기존 lint 25개 | 새 memo 변경 위치의 lint는 없었지만 전체 Clippy 통과로 기록하지 않음 |
| Windows WASM typecheck | `curve25519-dalek` build-script 실행 전 OS error 4551로 차단 | 비게시 Linux CI의 현재 source 결과 필요 |

두 감독 영수증 `1790504677533664500-27992-runtime.json` 및
`1790504878348159600-34428-runtime.json`은 pressure 0, root 테스트 성공 후
`descendant_processes_at_exit=1`, `reason=tree-not-stopped`, return 125다.
cleanup 이후 `process_tree_stopped=true`이며 이를 정상 감독 종료 또는 OOM으로
다시 분류하지 않는다. 감독기는 정상 root 종료 시 configured grace를 사용하지 않고
collector join 뒤 남은 tree를 바로 강제 정리한다. 정확한 descendant 원인과 finite
post-exit drain은 별도 미완료 항목으로 유지하며 이번 memo 수정에서 완화하지 않는다.

기존 conditioned context 준비 캐시는 이미 frame/piece 단위로 존재한다. 후속 비용
분리는 이 준비를 반복하는 것으로 가정하지 않고 board별 prepared-record 조회와
miss 통과, hit의 exit composition을 구분해야 한다. union memo의 두 node ID는
worker/epoch-local namespace이므로 동일 numeric key만으로 전역 공유하지 않는다.

다음 검증은 새 binary에서 flat/adaptive 11-worker P7P4 및 작은 P7을 같은 실행
조건으로 비교하고 migration peak와 실제 전환 수를 함께 확인하는 것이다. 이미
완료한 강제 State-major 및 두 accelerator의 이전 ABBA는 재실행하거나 새 정책의
성능 증거로 재명명하지 않는다. v0.8.1의 p95/shared-memory/surface/release gate는
여전히 Open이며 main, Pages, 4194, v0.9.0 TB는 이 변경으로 갱신하지 않는다.

## 동적 State-major의 실제 native 완주 검증

solver 소스 `abc5230a43800686152bacd25b022be322fb2db8`의 비게시 CI
[`36313061153`](https://github.com/daejunnom/Clearra/actions/runs/36313061153)는
Core, native-products, wasm-abi, surfaces 네 job 모두 성공했다. Linux WASM typecheck
및 bounded ABI 계약의 증거이며 실제 브라우저 runtime 또는 release acceptance가 아니다.
이후 하네스/문서 변경은 solver Rust 소스를 바꾸지 않는다.

새 로컬 native binary SHA256은
`a1cbf982538fc3f562b527f72fcff0472e46b12c6d3b24ef513bd15fe0a86b25`다.
같은 binary로 11-worker 작은 P7의 ABBA+BAAB와 빈 필드 4L SRS+ P7P4 ABBA를
순서대로 실행했다. TB, legal-board, conditioned relation은 모두 비활성했다.
비교 중 binary와 하네스 해시는 바뀌지 않았으며 tracked Rust patch는 없었다.

| 입력 / arm | 완료 수 | 평균 supervisor-start-through-exit | 평균 aggregate commit peak | 동적 전환 증거 |
| --- | ---: | ---: | ---: | --- |
| 기존 필드 P7 / flat | 4 | 92.705ms | 67.620MiB | flat, directory 0 |
| 기존 필드 P7 / adaptive | 4 | 93.972ms | 67.759MiB | flat, attempts/promotions/directory 모두 0 |
| P7P4 / flat | 2 | 121.009s | 6.466GiB | flat, promotions 0 |
| P7P4 / adaptive | 2 | 117.548s | 5.830GiB | 각 표본 attempts 11, promotions 11, 실제 State-major |

모든 timed sample은 실제/활성 보고 워커 11, CPU parallel 실행, 정상 inner 감독 종료와
tree 정지를 통과했다. 작은 입력은 246개와 `cts1:cb0b19c391d5003e`, 후보 집합 digest
`270b7d6056dacf2d`가 일치했다. 큰 입력은 456,923개와 `cts1:98ebe8726537b29f`, 후보
집합 digest `d6715a89054ef642`가 일치했다. 이 집합 증거를 전체 canonical 출력 순서
또는 모든 surface parity로 확대하지 않는다.

큰 입력의 평균 commit peak는 9.839%, private exit retained bytes는
5.579→5.140GiB로 7.865% 감소했다. product memo payload는 평균 1,232→763.465MiB이며
adaptive directory 5.586MiB를 포함하면 37.577% 감소했다. union payload 평균은 두 arm
모두 2,352MiB로 남아 있다. shared immutable request table은 36,272 bytes를 한 번만
계수하며 worker 수를 곱하지 않는다. 이 지표는 asset active-session 128MiB gate와 별개다.

작은 입력 pressure는 8회 모두 0이다. 큰 flat 표본은 각각 3/2회, adaptive는 0/0회였고
outer Node supervisor에도 pressure 4회와 해당 GC acknowledgement 4회가 있었다.
outer/inner 이벤트는 중복 가능성이 있어 합쳐 unique pressure 횟수로 표현하지 않는다.
따라서 관측 평균 시간의 2.861% 개선, 작은 입력의 1.367% 차이는 p95 성능 채택 권위가
아니다. 적은 표본 수와 압력 차이 때문에 기존 gate는 Open이다. 과거 forced State-major
및 accelerator ABBA는 이 측정의 arm 또는 표본으로 섞지 않는다.

Cargo는 17m43s에 release 컴파일을 완료했지만 build 감독 영수증
`1790515946486317800-37788-runtime.json`은 root 종료 뒤 descendant 1개를 이유로
`tree-not-stopped`/125를 기록했다. 정리 후 `process_tree_stopped=true`이고 실제 Cargo,
rustc, solver 프로세스가 남지 않았음을 확인했다. build pressure 7회는 모두 회복됐지만
non-Node Cargo의 GC acknowledgement는 0이다. 실패 영수증을 정상 빌드 acceptance로
재분류하거나 resource 변경 재시도하지 않았다. 새 binary는 로컬 비교용이며 runtime
identity도 `unverified-local-build`다. 정상 root 종료 뒤 finite drain 검증은 별도 Open이다.

원시 manifest/sample/CLI JSON과 검토 보고서는 선언된 로컬
`_local/artifacts/v081-adaptive-memo-abba-20260927/`에만 보관한다. 이번 변경은 실제
동적 전환과 메모리 방향을 검증한 상태이며 release, main fast-forward, 4194 갱신이나
v0.9.0 TB 재개를 의미하지 않는다. 후속 우선순위는 worker/epoch-private union memo의
크기 비용과 conditioned miss/exit composition 비용을 분리해 개선하는 것이다.
