# 일반 큐 재사용·메모리 회계·worker 준비 후속 최적화

## 1. 복구한 기준과 범위

사용자가 제공한 목표의 100% 확률은 확정 전제다. 이 작업은 그 확률을 추가로
검색·탐색·증명하지 않는다. 실제 전체 열거의 완료 여부와 구현 성능은 별도
상태로 기록한다.

GitHub `codex/full6l-p1p7p7-20261122`의 `92dc86b0`와 이전 로컬 작업공간
`Clearra-resumed`의 `5f5adcfd`는 같은 tree
`f206496e48eec09a32d0ee87dc2480eb42408664`였다. 최신 구현과 측정은
[일반 큐·병렬 실행 기록](general-queue-parallel-numa-2026-11-23.md)에 있었다.
첨부 프로젝트 핸드오프는 초기 장기 설계이므로 현재 구현 완료표로 해석하지 않았다.

이번 성능 측정의 원격 제품 commit은 `ea3dc26faf05293cd625e1463de21133c91ae567`이며,
검증된 로컬 commit `44c9dad7135fe055912eaa87baa003525e59ec55`와 같은 tree
`a39988f2901b8d842da5e2665fc4923391e1d032`다. 검증 때의 로컬 commit
`662a02e0`를 동일 tree인 원격 기준 위에 재배치했고, 원격 기록에서도 그 tree를
다시 확인했다. 소스 내용과 검증 실행파일은 바뀌지 않았다. 후속 문서 기록 이후의
FFI 파일 경계, 데스크톱 잠금 파일, 전체 CI에서 발견한 계약·fixture 정리는 아래
8절에서 별도로 구분한다.

옛 작업공간의 미커밋 자료도 읽기 전용으로 확인했다. 큰 WebGPU 제거·생성 한도
정리는 현재 구현에 계승되어 있었고, 미연결 `extended_build_symbolic.rs` 초안은
현재 `extended_build_probability.rs`의 정식 경로에 통합되어 있었다. 초기 two-bag
진단은 현재 `clearra-app/examples/oracle_ctk3_bench.rs`가 계승한다. 별도의 높이
5·6 fixture 초안은 보존했으며, 현재 5·6L 지원과 모순되는 옛 거절 기대를 그대로
복원하지 않았다. 옛 소스와 실행 근거는 삭제하지 않았다.

## 2. 의존 그래프와 공급 계산의 공통 인수

[coverage_product.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/coverage_product.rs)는
이미 정확한 물리 BuildOrderGraph 또는 허용된 종류 언어와 원본 source/Hold를
교차한다. 원본 bitmap word를 w, graph 깊이를 d, 초기 source cursor를 q라 하면
공급 조회는 다음 함수로 분리된다.

```text
M(d,e,k,w) = source의 q+d+e 위치가 종류 k인 원본 ordinal bits
e ∈ {0,1}

Current/Swap: A AND M(d,e,k,w)
Store:        A AND M(d,1,desired,w) AND M(d,0,current,w)
```

`M`은 같은 depth·word 안에서 graph node, held kind, physical realization과
무관하다. Empty Hold의 Store는 extra_draw=0에서만 가능하며, 그 다음 위치는
extra_draw=1인 상태의 Current 위치와 같다. 따라서 두 위치의 7종 word를 lazy로
공유하면 충분하다. 새로운 source 전체 순회나 후보별 heap allocation은 없다.

**재사용이 켜진 `(depth, 원본 bitmap word)` 쌍**에서 Current·Swap·Store에 필요한
서로 다른 공급 인수 helper 계산은 최대 14개다. Standard6 projected complement는
helper 안에서 앞선 위치를 읽을 수 있으므로 실제 배열 읽기까지 14회 이하라는
뜻은 아니다. Hold가 꺼져 있고
해당 층의 product state가 하나뿐이면 이미 존재하는 depth queue 길이만 보고 직접
조회한다. 이 경로에서도 구체 pose edge와 CountAll의 labeled history는 전부 유지한다.
직접 조회 층이나 기존 terminal held-release helper까지 포함해 전역 조회 수가
14 이하라고 주장하지 않는다.

Cache에는 source word만 저장한다. 각 상태의 active ordinal bits는 매번 다시
AND하므로 서로 다른 의존 상태나 Hold 후속 상태를 합치지 않는다. 초기 cursor의
overflow 검사, finite EOF held-release, Standard6의 projected complement, finesse
spawn guard와 원본 범위도 기존 owner가 계속 검증한다.

새 분기·합류 fixture는 explicit 130원본, partial factorized source, 초기 cursor와
Hold, Boolean/CountAll, 같은 operation의 여러 pose를 독립 labeled-history oracle과
대조한다. 이전 전이 재사용만 적용한 기준과 비교해 source position-word 조회가
**1,524 → 972회(36.22% 감소)**였다. Coverage, path count, completeness, active
states, edge checks, transition 횟수와 retained heap bytes는 같았다. 이 수치는
조회 작업량이며 실행 시간 감소율이 아니다.

추가 공간은 평가 호출의 작은 stack 객체다. Word 14개는 112 bytes이며 borrowed
source 참조, 위치·word index, loaded tag와 전략 상태의 고정 metadata가 더해진다.
Retained heap 동일성과 stack 공간 증가를 구분한다. 작은 입력의 초기화·분기
비용은 아래 연산량 분석으로 한정하고, 추가 실측을 진행하지 않는다.

이 재사용은 materialized bitmap product와 rooted language adapter의 공통 공급
연산에 적용된다. 특정 P1P7P7 inventory를 조건으로 삼지 않는다. Full6에서
`StandardBagCoverage`의 symbolic owner로 처리하는 부분은 이 bitmap cache의
직접 적용 대상이 아니다. 해당 경로는 memo 개선과 실제 큰 입력 관측으로 따로
평가하며, 위 조회 감소율을 Full6 전체 가속률로 해석하지 않는다.

## 3. 절약한 메모리를 유지하며 회계 비용 제거

[standard_bag_product_memo.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_product_memo.rs)의
StateMajor 경로를 다음과 같이 정리했다.

- 삽입의 범위 확인·admission·row 준비에서 같은 row index를 한 번만 계산한다.
- 이미 존재하는 row를 다시 준비하지 않는다. 실제 성장 때만 reserve와 capacity
  전후 회계를 실행한다. Allocation이 없어도 admission callback은 유지한다.
- Migration 첫 pass에서 정확한 population을 reserve했으므로 두 번째 복사 pass에서
  entry마다 capacity를 다시 합산하지 않는다.
- Clear가 capacity를 유지한다는 계약에 따라 누적 capacity를 재사용한다. 이미 빈
  StateMajor memo를 다시 clear할 때는 directory 전체를 순회하지 않는다.

Entry 폭, directory, full-key equality, hash, 승격의 최소 12.5% 절약 기준,
logical-live recycle 경계, epoch와 실패 시 exact fallback은 그대로다.
Compact 12B map의 lookup에는 새 raw table이나 unsafe를 도입하지 않았다.

Flat에서 reserve 호출을 생략한 첫 실험은 작은 compact 입력에서 큰 지연 증가가
나타나 철회했다. 최종 flat 경로는 기존 reserve 호출을 유지한다. 두 실제 memo
모듈의 synthetic ABBA는 warmup 후 8 round, arm별 16표본을 사용했다.

| 입력 | 이전 중앙값(ms) | 변경 중앙값(ms) | 관측 |
| --- | ---: | ---: | --- |
| StateMajor 262,144 entries × 4 epochs | 43.471 | 42.992 | 1.10% 감소 |
| Adaptive 524,288 entries × 2 epochs | 62.154 | 61.524 | 1.01% 감소 |
| 작은 Adaptive 1,024 entries × 1,024 epochs | 16.501 | 16.756 | 1.55% 증가 |
| 작은 Compact 1,024 entries × 1,024 epochs | 17.9935 | 18.280 | 1.59% 증가 |

128회 모두 checksum, capacity, payload, directory bytes와 승격 횟수가 같았다.
표본 변동과 작은 증가를 숨기지 않으며 이 microbenchmark로 전체 검색의 가속이나
작은 요청의 무회귀를 확정하지 않는다. Receipt `1795378602810927944-5-runtime`은
exit 0, sampled RSS peak 19,091,456 bytes, pressure 0, automatic retry false다.

## 4. 준비된 worker의 실행과 NUMA

[cpu_worker_pool.rs](../../crates/clearra-core-executor/src/cpu_worker_pool.rs)의 기존
즉시 투입 정책을 유지하면서, 증설 중 요청이 겹칠 때 하나의 coordinator가 가장 큰
요청 target을 받도록 했다. Ready worker와 caller는 나머지 worker 생성을 기다리지
않는다. Target 갱신과 coordinator 종료 판정을 같은 짧은 mutex 안에서 처리하여
완료 직전의 증설 요청이 유실되지 않는다. 이 mutex는 worker 생성이나 실제 job 실행
중에 유지하지 않는다. Warm pool에는 기존 atomic 빠른 경로가 남는다.

한 worker만 요구하면 pool 생성과 topology 발견을 생략한다. Late spawn 실패에도
이미 발행한 logical job은 queue에 남고 준비된 worker가 처리한다. 요청 worker 수나
원본 작업 집합을 줄이는 변경은 아니다.

이전의 native NUMA 배치와 first-touch 경계를 계속 사용한다. 단일·불명확 topology는
OS scheduler로 돌아가고, main thread의 affinity를 바꾸지 않는다. 이 환경은 허용
논리 CPU 0–8, memory node 0이며 실제 다중 NUMA 성능 검증 환경이 아니다. 이번
coordinator 개선을 다중 NUMA에서의 실측 가속으로 표현하지 않는다.

## 5. Chromium host의 독립 대기와 요청 root

[ClearraVerifierPool.ts](../../apps/clearra-web/src/workers/ClearraVerifierPool.ts)에서
세 독립 구간을 겹쳤다.

1. 별도 accelerator slot인 pack과 synopsis의 설치·제거 ACK를 함께 기다린다.
2. Payload SHA-256과 durable authority 복구를 함께 시작한다.
3. 결과 봉인용 merger-ready digest와 worker-reply digest를 함께 계산한다.

각 구간의 양쪽 완료 후에만 기존 initializer, permit prepare 또는 result seal로
진입한다. Prepare/offer/publish/start/commit 순서는 유지한다. 새 worker나 장기
cache를 추가하지 않으며, hashing 자체의 단기 작업이 겹치는 것은 구분한다.
Digest 입력과 임시 버퍼가 동시에 살아 있을 수 있으므로, 장기 cache를 추가하지
않았다는 사실을 Chromium peak memory 무증가의 보장으로 해석하지 않는다.

동일 warm worker를 새 요청으로 초기화할 때 permit이 이전 request SHA를 참조하던
문제도 수정했다. Initialize는 항상 현재 payload SHA를 새 root로 사용하고 후속
consume/finish가 그 root를 따른다. 명시적인 gate를 쓰는 새 회귀로 ACK·hash 대기
순서와 두 요청의 모든 journal event를 검증했다.

실제 Chromium orchestration는 page/worker 실행 전
`process_singleton_posix.cc: socket() failed: Operation not permitted`로 종료됐다.
Receipt는 `1795378424825023867-7-runtime`이다. 권한 우회나 플래그 재시도는 하지
않았다. Node 계약·TypeScript·WASM compile을 실제 Chromium 성능 통과로 대체하지 않는다.

## 6. 검증 및 실행 측정

| 검증 | 결과 |
| --- | --- |
| Core 집중 회귀 | 38 PASS, 0 FAIL; 1.08초 |
| 독립 memo 회귀 | 원본+변경 39 PASS, 최종 변경 20 PASS |
| Browser 계약 | Pool, durable journal, distributed runner, host yield 4개 실행 통과 |
| Focused strict TypeScript | TS 5.9.3, 실제 imported closure 73개, diagnostics 0 |
| WASM32 check | PASS, 32.80초; 기존 dead-code 경고 7개 |
| Rustfmt / diff whitespace | PASS |

Node 계약 실행 환경은 Node 24.22.3이다. Focused TypeScript는 저장소 contract
설정과 typed Node shim을 사용했으며 전체 Svelte 앱 typecheck와 구분한다.
성능 비교의 source·binary와 결과는 아래 최종 측정 절에 기록한다.

### 6.1. P7P4 전체 열거 ABBA

고정 입력은 기존 `wasm_cpu_parallel_exact`의 P7P4 Unique 테스트다. 각 실행의
실제 참가자는 8명이며 이 테스트 정책과 Full6 Auto 9명을 구분한다. 네 실행 모두
**456,923개**, **`cts1:98ebe8726537b29f`**, complete와 parallel 계약을 통과했다.

| Arm | Test 시간(초) | Sampled RSS peak(bytes) | GiB | Receipt ID |
| --- | ---: | ---: | ---: | --- |
| A1 이전 | 69.97 | 4,113,567,744 | 3.8311 | `1795379079684380390-99` |
| B1 변경 | 68.50 | 4,064,047,104 | 3.7849 | `1795379149749399029-118` |
| B2 변경 | 69.96 | 4,195,909,632 | 3.9077 | `1795379218370025812-137` |
| A2 이전 | 73.88 | 4,214,812,672 | 3.9254 | `1795379288404569005-156` |

시간 중앙값은 **71.925 → 69.230초**, 이 입력의 관측 감소는 **3.747%**다.
Sampled peak 중앙값은 **4,164,190,208 → 4,129,978,368 bytes**, 0.822% 낮았다.
네 receipt 모두 normal/exit 0, pressure event 0, automatic retry false다.
Peak는 `linux-sampled-process-group-rss`이며 kernel high-water가 아니다. 두 표본씩의
관측이고 여러 변경을 함께 포함하므로 개별 변경의 가속률이나 일반적 성능 보장으로
확대하지 않는다.

### 6.2. 작은 입력은 연산량만 확인

사용자의 최종 우선순위는 아주 작은 입력의 지연보다 큰 입력의 개선이다. 작은
입력의 추가 실측과 그 수치에 맞춘 별도 구조 변경은 중단했다. 지시 이전에 완료된
fresh-process, in-process, profile 및 같은 실행파일 control의 원본 로그는
보존했다. 최초 in-process에서 관측한 약 11% 증가는 긴 반복에서 같은 방향으로
재현되지 않았고, 같은 실행파일끼리도 차이가 있었다. 이를 확정 회귀 또는 특정
cache 변경의 인과 효과로 기록하지 않는다.

코드상 비용은 다음과 같다.

- 평가 호출마다 14개 word와 고정 metadata를 초기화한다. 새 heap 할당이나 큐·graph
  전체 prepass는 없다.
- 각 `(depth, word)`에서 두 loaded tag와 위치·word metadata를 갱신한다. 비어 있는
  기존 depth queue를 지나는 경우에도 이 상수 작업은 남는다.
- Hold가 꺼진 단일 상태 층은 전략 분기 후 기존 source 조회로 바로 간다. Cache
  유효 bit 검사와 저장은 생략하지만 초기화·분기 비용이 정확히 0인 것은 아니다.

큰 입력에서 캐시가 켜진 한 `(depth, word)`의 공급 인수 helper 요청을 Q회,
실제로 요청된 서로 다른
`(position, kind)`를 U개라 하면 기존 원본 조회 Q회가 U회로 줄고, `U ≤ min(Q, 14)`다.
대신 Q회의 작은 cache 검사와 U회의 저장이 남는다. 원본 조회 비용을 C라고 하면
그 구간의 비용은 `O(Q × C)`에서 `O(U × C + Q)`가 된다. 실제 graph 전이, active
ordinal 교집합과 CountAll의 실행 이력 수는 유지된다. 전체 탐색의 복잡도 차수가
바뀐다는 뜻은 아니다.

### 6.3. Full6 P1P7P7의 240초 진단

최종 제품 실행파일을 240초 cooperative cancellation, 270초 supervisor hard
timeout으로 한 번 실행했다. Width 10, empty height 6, SRS+, 15 locks, empty Hold,
FullQueueOracle, CountUnique와 전체 177,811,200개 원본의 계약 검사를 통과했다.
Chance evidence는 Disabled, solution probability는 Omit이며 원본·node·candidate·
frontier의 제품 생성 한도를 추가하지 않았다. 이 실행은 100% 확률에 대한 탐색이나
추가 증명이 아니다.

| 항목 | 관측 |
| --- | ---: |
| 요청 참가자 / task split | Auto 9 / 9개 |
| Family compile | 116.558초 |
| Family compile 및 준비·split span | 118.212초 |
| Family expanded states | 13,304,012 |
| Family nodes | 52,325,237 |
| 원시 geometry family paths | 247,794,216,021,372 |
| 취소까지 emitted candidates, caller + pool | 12,781,308 |
| 프로그램 총 경과 시간 | 240.820초 |
| Supervisor sampled process-group RSS peak | 5,947,895,808 bytes, 5.5394 GiB |
| 프로세스 kernel RSS high-water | 5,947,514,880 bytes |
| Memory pressure events / automatic retry | 0 / false |

Family 준비 후 path-count pass는 1.607초, pooled worker의 row-order 준비 pass는
1.355초였다. 이 독립 pass들은 겹쳐 실행되므로 둘을 더해서 준비 wall time으로
표현하지 않는다. 9개 task 분할과 caller·pooled worker의 실제 작업 계측이 남았다.

탐색 중 누적 profile에서는 물리 build-order/reachability가 caller 97.984초,
pooled workers 합계 323.715초로 가장 큰 축이었다. Coverage language product는
각각 0.769초와 92.147초였다. Profile은 겹칠 수 있는 중첩 span이며 일부 duration은
sampling 추정치다. Worker 누계는 wall time이 아니고, 위 값을 합산하거나 240초로
나누어 비중·전체 완료 시간을 계산하지 않는다. 실제 병목이 공급 mask 조회에만
있지 않으며, 큰 물리 의존 그래프 준비·검증을 다음 대상으로 삼아야 한다는 근거다.

최종 상태는 **`complete=false`, `native Rust PC: Cancelled`**다. 240초 진단 취소에
협조적으로 종료했고 receipt의 exit 1은 이 미완료 상태를 반영한다. Supervisor의
강제 종료, OOM, 자원 변경 재시도는 없었다. 원시 paths와 emitted candidates는
성공 해답 수가 아니며, 최종 Unique count 또는 전체 결과 hash를 얻지 못했다.
이번 실행을 Full6 완주나 이전 버전 대비 Full6 가속 측정으로 기록하지 않는다.

Receipt는 `1795380028297435399-5-runtime`, 원본은 `full6-after.json`과
`full6-after.stderr`다. 이번에는 현재 버전의 병목 진단만 수행했다. 준비해 둔 이전
Full6 실행파일을 재실행하여 불완전한 두 표본의 속도 비교를 추가하지 않았다.

### 6.4. 실행파일과 재현

모든 Rust 실행은 1.98.1이며 제품 코드에 `local-search-ab`를 켜지 않았다.
이전 P7P4 binary의 source는 `e6dc9377`(원격 동등 코드 `dd36cad4`), 이전 Full6
binary는 같은 코드의 문서 후속 `5f5adcfd`다. 변경 binary는 위 1절의 같은 tree다.

| 실행파일 | SHA-256 |
| --- | --- |
| P7P4 이전 | `d4913e748d33985ac79028cc31aafb77b3dbf4b25663ba331feb681b9cf6fc51` |
| P7P4 변경 | `6c731c5c94ce2a7f647a366edf9b1d77ca7448421d9d9af0cf3b7264618ca0f0` |
| Full6 이전 | `dc369bc755f165622e3889ed1ba7859a2ade038f2be0342f99850b6018707fcf` |
| Full6 변경 | `c73d5c0f3e7656631b61b8f2334298efe51f79334ab76fee4204cbaa435ac21d` |

Raw 실행 자료와 Rust 집계 helper는 `_local/artifacts/general-queue-perf-20261123/`,
Browser 자료는 `_local/artifacts/browser-latency-20261123/`에 있다. 이 문서는 그중
검토한 결과를 기록한다. 초기 ENOSPC 빌드 실패는 성능 표본에 넣지 않았고, 선언된
옛 build root의 재생성 가능한 incremental cache만 검토 후 정리했다.

```sh
cargo test --locked --offline -p clearra-core-executor \
  --features parallel,wasm-stage-profiling --lib -- \
  coverage_product standard_bag_product_memo \
  standard_bag_coverage::supply_reuse_tests cpu_worker_pool --test-threads=1

cargo test --locked --offline --release -p clearra-core-executor \
  --features parallel,wasm-stage-profiling \
  --test wasm_cpu_parallel_exact --no-run

clearra-manage runtime run --producer queue-p7p4-reproduce \
  --profile benchmark-search --timeout 180 -- <built-integration-binary> \
  p7p4_parallel_matches_exact_solution_oracle --exact --ignored \
  --nocapture --test-threads=1

clearra-manage runtime run --producer queue-full6-reproduce \
  --profile benchmark-search --timeout 270 -- <built-full6l-bench> \
  run --objective unique --profile --cancel-after-seconds 240

cargo check --locked --offline -p clearra-core-executor \
  --features parallel,wasm-stage-profiling --target wasm32-unknown-unknown --lib
```

## 7. 남은 작업

Full6 전체 후보 영역 소진과 수 분 내 완주는 아직 달성하지 못했다.
이번 진단에서도 약 5232만-node family 생성과 큰 물리 BuildOrder/source product가
핵심 경계다. 다음 구현은 `geometry.rs`의 공유 residual 의존성을 보존하는 준비
작업 분할, `buildup.rs`와 종류 언어의 같은 물리 계산 재사용, 실제로 비용을 회수하는
일반적인 음성 가지치기를 대상으로 해야 한다. Thread만 늘리면서 대형 family를
worker별로 복제하는 방식은 메모리 목표를 충족하지 않는다.

완료한 결과의 전체 identity·canonical key 보관 비용도 별도 과제다. 정렬·중복 제거·
hash·전체 결과 조회 계약을 보존해야 하며, 부분 count나 단순 hash 합으로 전체 열거를
대체하지 않는다. 실제 Chromium 및 다중 NUMA/Windows 환경에서의 실측도 남아 있다.

새 Rust 코드는 [Google Rust Style Guide](https://google.github.io/styleguide/rustguide.html)의
borrowing, 명시적인 소유 범위, 단순한 자료구조와 설명형 실패 문맥, rustfmt를 따른다.
Chromium 코드베이스 전용 thread 지침을 이 Cargo 프로젝트의 표준 thread 금지로
확대하지 않는다.

## 8. 후보 CI에서 확인한 통합 누락 정리

보고서를 포함한 `a03a4b9bc6d9c4f1da0c5020355aa72aa3f2e14e`에 대해
[Candidate Preflight #57](https://github.com/daejunnom/Clearra/actions/runs/45068762194)를
한 번 실행했다. `no-product-debt`의 unsafe architecture 검사가 앞선 `dd36cad4`에서
추가된 두 OS affinity 파일의 위치를 거절했다. 두 파일은 `clearra-core-ffi` 안에
있었으나 지정된 raw 경계 밖에 있었다.

OS ABI 구현을 다음 기존 raw 디렉터리로 옮기고 안전한 facade의 `#[path]` 두 곳을
수정했다.

- `crates/clearra-core-ffi/src/raw/native_thread_affinity_linux.rs`
- `crates/clearra-core-ffi/src/raw/native_thread_affinity_windows.rs`

두 플랫폼 파일의 내용은 byte 단위로 동일하다. 논리 모듈은 계속
`native_thread_affinity::platform`이며 `pub(super)`, `super` 참조, 플랫폼 cfg,
안전한 공개 API와 OS 호출 계약도 같다. `raw/mod.rs`의 중복 등록은 필요 없다.
기존 validator와 unsafe 허용 목록은 변경하지 않았다.

기존 affinity 단위 테스트 3개와 rustfmt를 통과했다. 이 로컬 환경에는 PowerShell이
없어 실제 architecture task의 실행 확인은 후속 정확한 커밋의 Candidate Preflight에
맡긴다. 6절의 성능 수치는 FFI 파일 이동 이전의 명시된 실행파일에 대한 값이며,
파일 이동 후 새로운 소입력 성능 측정은 추가하지 않았다.

같은 후보의 DesktopHost 제품 검사는 `status=passed`였지만 build transaction의
마지막 입력 서명 검사에서 `source_changed=True`, `input_count_changed=False`,
`context_changed=False`로 실패했다. 이 서명은 입력 파일의 상대경로·길이·내용
SHA-256을 사용하므로 단순 mtime이나 원격 브랜치 갱신이 원인은 아니다.

확인된 구체적 불일치는 `dd36cad4`의 Linux `libc` 의존성 추가가 루트 Cargo.lock에만
반영되고 독립 `apps/clearra-desktop/src-tauri/Cargo.lock`의 `clearra-core-ffi` dependency
목록에는 빠진 점이다. 데스크톱 lockfile에는 동일한 `libc 0.2.186` 패키지가 이미
있으므로 누락된 연결 한 행만 추가했다. `desktop-host-check.ps1`의 Cargo test/check에
`--locked`를 추가하여 향후 불일치를 컴파일 전에 직접 거부하게 했다. 빌드 입력
서명과 transaction seal 계약은 그대로 유지한다.

이는 관측된 서명 변화와 일치하는 원인 후보지만, 최초 CI는 바뀐 파일별 manifest를
남기지 않아 그 실행의 변경 파일을 사후 확정하지 못한다. 로컬의 `--locked --offline`
metadata 시도도 Tauri 독립 의존성의 registry `serde_json`이 없어 해석 전에 막혔으므로
stale lock 오류를 직접 재현했다고 기록하지 않는다. 수정된 잠금 파일과 실행 명령은
후속 정확한 커밋의 Windows DesktopHost 및 transaction seal 결과로 확인한다.

### 8.1. 전체 Rust 검증에서 확인한 계약과 fixture

최초 후보의 RustExact는 app 483 passed / 5 failed / 2 ignored, core executor
1414 passed / 2 failed / 19 ignored였다. 실패를 다음과 같이 구분하여 수정했다.

- `rust_legacy_cover.rs`의 진입 guard가 `BuildCoverageLimits`의 **0 = 무제한**
  계약과 달리 0 budget을 거부했다. 양수 pattern limit을 넘을 때만 거부하도록
  수정하고, assignment limit 0 거부를 제거했다. 빈 원본 축, 범위 밖 selected
  ordinal, 명시적인 유한 pattern cap의 거부는 유지한다. 각각의 0 sentinel을
  독립 확인하고 유한 assignment budget 소진 시 incomplete 결과를 완전한
  coverage로 발행하지 못하는 회귀 검증을 추가했다. 기존 app 실패 4개가 이
  guard의 영향을 받았다.
- `warning_allows_execution`은 제거된 GPU fallback을 성공 경로로 기대했다.
  현재 CPU 경로의 observed-supply 경고로 fixture를 바꾸고, 정확한 경고 코드,
  실제 solver 실행·성공 결과, fallback 미사용까지 확인한다.
- Truncated original-axis fixture는 retained 원본에 witness가 없는데도
  `covers_any_pattern=true`를 기대했다. 완전한 원본 축의 dense/symbolic 양성
  대조군을 추가하고 truncated 축의 false·0·no witness·empty bits를 확인한다.
  제품 구현을 수정한 것이 아니라 기존 retained-axis 계약에 기대값을 맞췄다.
- Full6 pruning gate fixture가 여러 search session의 compute/memory lease를
  동시에 보유하여 4 CPU CI에서 admission을 소진했다. 불변 catalog만 공유하고
  각 session의 수명을 개별 scope로 제한했다. 모든 gate assertion과
  `expanded_nodes == 0` 검증은 유지하며, 제품의 자원 admission을 완화하지 않았다.

두 core 실패는 이전 소스로 재현했다. 수정 후 각 정확한 테스트가 통과했으며,
Full6 gate fixture는 CPU 2개로 제한해도 통과했다. 이는 session 구성 검증으로,
거대한 family를 생성하거나 Full6 탐색을 재실행하지 않는다.

App 수정은 `cargo check --locked --offline -p clearra-app --tests`를 통과했다.
로컬 native-C libtest 빌드는 `clearra_core` 정적 archive 부재로 링크하지 못했고,
default app 전체 libtest 빌드는 codegen 중 SIGKILL로 끝났다. 두 경우 모두 테스트
실행 결과로 계산하지 않는다. 동일한 전체 libtest를 재시도하지 않고 production
library와 공개 API를 사용하는 좁은 기능 검증으로 로컬 확인 범위를 한정했다.
첫 default-only library는 native worker pool이 없는 feature 조합이어서 계산 전
거부됐다. 실제 native Rust 제품과 같은 `parallel` feature의 production library에
연결한 공개 API 검증은 **4 passed / 0 failed / 0 ignored**였다. Observed-supply
경고 후 실행, 양수 original ordinal, 기존 빈 packing 결과, 전체 original 축과
무제한 budget·유한 cap 거부를 확인했다. 이는 기능 검증이며 소입력 성능 표본이
아니다. 전체 native-C libtest와 같다고 간주하지 않고 후속 CI에 맡긴다.

정확한 library·harness source SHA-256·실행 인자는 `app-public-api-parallel-command.json`,
결과는 `app-public-api-parallel-functional.log`에 있다. Production library 빌드
receipt는 `1795382097353591410-5`, 공개 API 실행은 `1795382208652246596-5`,
unit source 타입 검사는 `1795381871870845074-5`다. 실패한 앞선 시도의 로그와
receipt도 보존했으며 결과 수치에서 제외했다. 담당 파일의 rustfmt와 전체 diff
whitespace 검사도 통과했다.

### 8.2. 최초 후보의 최종 결과와 후속 검증 범위

최초 run `45068762194`, attempt 1은 source SHA `a03a4b9b`에서 **실행 job
6개 성공 / 3개 실패**로 종료했다. Source, adversarial, sanitizer, Linux native
readback, WASM·Chromium, product E2E가 통과했고, no-product-debt, desktop-host,
rust-exact가 위 이유로 실패했다. Focused job 4개의 skipped는 full gate 선택에
따른 정상 분기다. 이 실패 run은 취소하거나 재실행하지 않았다.

수정 후 새 커밋의 전체 Candidate Preflight 결과는 해당 exact SHA의 별도 run으로
확인한다. 성능 수치는 계속 1절에 명시한 제품 tree와 실행파일에 귀속되며 후속
계약·fixture 수정 커밋의 성능 측정으로 재명명하지 않는다. Chromium 기능 검증
통과는 실제 지연 감소율이나 브라우저 peak memory 측정 결과를 의미하지 않는다.
원본 CI 응답·실패 로그와 로컬 회귀 근거는
`_local/artifacts/candidate-preflight-45068762194/`에 보존했다.

### 8.3. 후속 후보에서 확인한 feature·lockfile 해석

통합 수정 `01022f7dd96939c8c1b714ba3108255c40815f88`의
[Candidate Preflight #58](https://github.com/daejunnom/Clearra/actions/runs/45071469548)는
앞선 unsafe 경계 검사를 통과한 뒤 추가 누락 두 가지를 드러냈다.

`native_unavailable_explicit_error` 증거는 `parallel`이 없는 기본 feature의
core libtest를 컴파일한다. 이때 unconditional host-yield 회귀가 parallel에서만
정의한 `GeometrySearch::placeholder()`를 호출하여 E0599로 실패했다. 생성자의
cfg 한 줄을 `any(feature = "parallel", test)`로 넓혀 같은 초기 상태를 테스트에서도
사용하게 했다. 직렬 host-yield 회귀나 native-unavailable error assertion을
제외하지 않으며, 비parallel 제품 빌드의 API는 늘리지 않는다.

실패한 조합 그대로 `cargo check --locked --offline -p clearra-core-executor --tests`
가 통과했다. Cargo fingerprint는 `default, placement-dependency-pc`이며 parallel과
native-c-core는 없다. Receipt는 `1795382777952436637-5-runtime`, 로그는
`core-serial-placeholder-check.log`다. 추가 대형 libtest 링크·실행이나 성능 측정은
하지 않았다.

DesktopHost에서는 `--locked`가 실제 Tauri check의 lockfile 불일치를 명시적으로
거부했다. 앞선 `libc` 연결 보강만으로는 현재 manifest의 dependency closure와
일치하지 않았다. Cargo의 `update --workspace --dry-run`과 이후 offline workspace
해석으로 기존 버전을 보존하며 정리했다. 새 패키지나 버전 업데이트는 없고,
선택되지 않는 `bytemuck_derive 1.12.0`, `libm 0.2.16`, `objc2-metal 0.3.2`와
관련 feature dependency를 제거하며 유일한 버전의 dependency 표기를 정규화했다.
Manifest, `--locked`와 build-input seal 검사는 그대로 유지한다.

이어 Windows 대상의 다음 full-resolve가 exit 0으로 완료됐다.

```sh
cargo metadata --locked \
  --manifest-path apps/clearra-desktop/src-tauri/Cargo.toml \
  --filter-platform x86_64-pc-windows-msvc --format-version 1
```

`--no-deps`를 쓰지 않았고 CI와 같은 기본 feature로 의존성을 해석했다. Lock package는
476개에서 473개가 되었으며 신규·버전 변경은 0개다. 이는 lock consistency 검증이고
Windows Tauri 빌드 자체의 통과는 후속 CI가 확인한다.

이 후속 후보의 원본 CI 응답·실패 로그와 로컬 확인 자료는
`_local/artifacts/candidate-preflight-45071469548/`에 보존한다. 성능 측정 source는
계속 1절의 tree이며, cfg·lockfile 후속 수정의 성능 결과로 바꾸지 않는다.
