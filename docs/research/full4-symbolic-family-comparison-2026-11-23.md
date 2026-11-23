# Full 4L / P7P4 기존 경로와 직접 symbolic family 비교

## 1. 이번 실행의 결론

2026-11-23에 빈 4L / P7P4를 같은 요청과 같은 최적화 바이너리로 순차 실행했다.
**두 경로 모두 완료 전에 SIGKILL로 종료됐다.** 완료 시간의 개선율과 Full 4L
전체 해법의 정확성 동등성은 아직 판정할 수 없다. 종료까지 걸린 시간을 완주
시간으로 사용하거나, 출력하지 못한 집합을 공집합으로 취급하지 않는다.

현재 구현에서 확인한 개선 대상은 두 부분이다.

1. 새 geometry 진입점은 기존 residual memo와 component composition을
   사용하지 않는다. 준비 완료 경계가 기존 약 0.403초, symbolic 약 99.952초였다.
2. Symbolic은 기하 준비 뒤 첫 배치 단계를 완료했지만, 두 번째 단계의
   `row_down`에서 저장소가 빠르게 커졌다. 마지막 3초에는 같은 문맥과 work 표본을
   유지하면서 RSS가 약 1.71 GB에서 2.79 GB로 증가했다.

이번에는 두 알고리즘을 수정하지 않고 관측과 검산 경계만 추가했다. 결과는 현재
프로토타입의 구현 상태를 평가하는 자료다. 원래 DAG를 직접 처리하는 설계의 최종
성능이나, 최적화된 동일한 DAG 표현을 인계했을 때의 비용을 확정하지 않는다.

배경은 [직접 symbolic family 구현 기록](direct-symbolic-family-implementation-2026-11-23.md)과
[Full 6L 직접 family 평가 설계](full6l-direct-family-evaluation-2026-11-23.md)에 있다.

## 2. 동일하게 고정한 조건

양쪽 모두 새
[`symbolic_family_compare` example](../../crates/clearra-core-executor/examples/symbolic_family_compare.rs)의
같은 `compile_problem()`을 사용한다. Opening preset과 Scenario preset을 섞지 않았다.

| 항목 | 고정한 값 |
| --- | --- |
| 보드 | 빈 10×4, 정확히 10 locks |
| 규칙 | Jstris 180, verified builtin ordered kick table |
| 원본 공급 | 실제 `Standard7Bag`, 11 draws; 언어가 P7P4인 source |
| 원본 큐 수와 순서 | 4,233,600; `StandardBagOrdinals`, `IOTSZJL` 순서 |
| Hold / 관측 | 허용, 처음에는 비어 있음 / `FullQueueOracle` |
| 목표 | `CountUnique` / `unique()` |
| 출력 | `Summary`, 해법별 확률 `Omit`; 전체 identity 집합과 global coverage 유지 |
| CPU 요청 | Auto, 모든 허용 논리 CPU, hardware limit 9 |
| 기존 경로 실제 worker 보고 | 정상 반환 전에 종료되어 최종 `workers_used`를 얻지 못함 |
| 새 경로 구현 | 하나의 공유 arena를 쓰는 직렬 탐색 |
| warmup / 선택적 가속 | CPU·GPU warmup, tablebase, exact legal-board, conditioned reachability 비활성 |
| 탐색 제한 | 시간·node·candidate·frontier·pattern 한도 0, memory limit `None` |
| Symbolic 표현 제한 | `SymbolicFamilyLimits::default()`, 모든 논리 한도 0 |
| 격리 | 같은 바이너리의 별도 fresh process, legacy 종료 후 symbolic 실행 |
| 관리 | `benchmark-search --no-timeout`, 메모리 감독 유지 |

컴파일된 `SearchProblemBudget`의 기본 `max_results = 256`도 fingerprint에
그대로 기록했다. 이를 전체 normalized identity 검산 개수로 사용하지 않는다.
하네스는 정상 반환 뒤 보고된 전체 unique count와 전체 추출 길이를 확인한다.

기존 `Omit` 경로는 전체 coverage가 확보된 뒤 후보별 Boolean witness만 검사할
수 있다. 새 경로는 현재 `(identity, 원래 큐)` 관계 전체와 pair count까지 유지한다.
같은 사용자 결과를 요청하지만 내부에 보존하는 정보량까지 같지는 않다. 이 차이와
직렬/병렬 구현 상태를 포함한 현재 구현끼리의 비교다.

### 빌드와 입력 식별

Rust는 `1.98.1 (48a229cea 2026-09-01)`이다. 첫 빌드부터 release `opt-level=3`,
`lto=false`, `codegen-units=16`, `incremental=false`를 사용했다. 당시 가용 RAM을
고려한 최초 빌드 설정이며 두 경로에 동일하다. 저장소의 기본 release 설정인
thin LTO / codegen-units 1과 구별한다. 빌드 OOM 뒤 자원을 바꾼 재시도가 아니다.
기본 `placement-dependency-pc`에 `symbolic-family,parallel`을 추가했다.
`local-search-ab`와 단계별 시간 profiler는 활성화하지 않았다.

| 식별자 | 값 |
| --- | --- |
| 구현 기준 commit | `85bce9bd0cce8b7487250c2079bfd782b5fe4ace` |
| 계측·하네스를 포함한 빌드 source tree | `17d749e03656caa57b6858fc4dec30ae35f716e4` |
| 실행파일 SHA-256 | `cf11c337602fb673c0cc017673f97bb5f75629409ba7ced8d3db77f9c512a938` |
| source fingerprint SHA-256 | `989d0aa152b2a75913d02142fcca6df8d6e88302de116c84060fb22decc81d56` |
| compiled request fingerprint SHA-256 | `e709af430f47ba7292eba224b3af7414efaa13560082dc77c474089992afa924` |

`compiled_source_commit`은 기준 HEAD이며 이후 계측 변경은 별도 source tree로
식별했다. 이 문서 추가는 측정 바이너리의 코드 변경을 뜻하지 않는다. 두 실행의
source/request 객체 자체와 fingerprint, 실제 실행파일 SHA를 Rust 관측 도구로
다시 비교해 일치를 확인했다.

## 3. 시간·메모리·종료 상태

| 관측값 | 기존 경로 | 새 symbolic 경로 |
| --- | ---: | ---: |
| 관리자 프로세스 실행 시간 | 18.730초 | 128.480초 |
| 준비 완료 경계, search 시작 기준 | 0.402721210초 | 99.951986695초 |
| 종료 상태 | SIGKILL 9 / exit 137 | SIGKILL 9 / exit 137 |
| `search_complete` | false | false |
| 관리자 2초 표본 RSS 최대 | 3,020,353,536 B / 2.813 GiB | 2,159,460,352 B / 2.011 GiB |
| 프로세스 1초 표본 VmHWM 최대 | 2,992,443,392 B / 2.787 GiB | 2,787,586,048 B / 2.596 GiB |
| 마지막 탐색 문맥 | 기하 family 준비 경계 이후, 세부 단계 미확인 | rank 2, S, catalog row 263, `row_down` |
| 전체 identity / global source 검산 | 실행되지 못함 | 실행되지 못함 |

두 준비 완료 경계의 내부 작업은 완전히 같지 않다. 기존
`geometry-candidate-family`는 기존 경로의 추가 준비를 포함하고, symbolic의
`symbolic_phase_occupancy`는 source·target·family 준비 뒤 점유식 작성 직전이다.
위 값을 같은 geometry 함수의 순수 실행 시간으로 설명하지 않는다.
기존 marker는 병렬 task 분할 직전에 발행된다. 마지막 marker만으로 실제 검증
worker가 일을 시작했는지까지 확정하지 않는다.

물리 RAM은 10,451,423,232 bytes, 약 9.73 GiB이며 swap은 없다. 시작 시 가용 물리
RAM은 기존 4,326,510,592 bytes, 새 경로 4,433,825,792 bytes였다. 이 값은 소유
프로세스의 사용 가능 메모리 상한을 보장하지 않는다.

두 receipt 모두 `timeout_seconds = 0`, `supervisor_termination_reason = null`,
`memory_pressure_events = 0`, `automatic_retry = false`다. 관리자가 시간 제한이나
자체 메모리 압력 규칙으로 종료한 실행은 아니다. 빠른 RSS 증가와 SIGKILL을 근거로
**OOM/메모리 압력을 우선 원인으로 취급**하지만, 커널 OOM counter와 실제 kernel
peak는 관측할 수 없었다. 정확한 OOM 임계값이나 마지막 순간의 최대 RSS는 확정하지
않는다. 2초 관리자 표본과 1초 프로세스 표본의 차이는 이 불확실성을 보여준다.

양쪽 모두 미완료이므로 종료 시간의 비율을 speedup으로 사용하지 않는다. 메모리
표본의 차이도 같은 완료 결과를 만드는 데 필요한 메모리 절감률이 아니다.
worker 수나 자원 설정을 바꾸는 OOM 재시도는 수행하지 않았다.

## 4. 관측을 소스에 연결한 병목 분석

### 4.1 원본 family 생성에서 빠진 기존 최적화

[`geometry.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs)의
`compile_symbolic_geometry`는 `FamilyCompiler::try_new`를 사용한다. 이 생성자는
`ResidualMemo::disabled()`와 `resource_authoritative = true`로 시작한다.
기존 무제한 생성자는 residual memo를 만들고 `resource_authoritative = false`를
사용한다. `advanced_analysis_enabled = !resource_authoritative && target_depth >= 7`
조건 때문에 새 진입점에서는 component composition도 꺼진다.

이번 빈 4L / 10-lock에서 확인한 주요 차이는 **잔여 상태 재사용과 컴포넌트 분해**다.
15개 미노용 inventory 최적화나 비어 있지 않은 보드용 Hall/exact projection을 이번
차이의 원인에 더하지 않는다. family interner가 한도 때문에 꺼졌다는 근거도 없다.

이 차이는 준비 시간뿐 아니라 다음 단계가 받는 DAG의 Product 분해·공유 정도에도
영향을 줄 수 있다. 동일한 최적화 family를 인계했을 때 Down 비용이 얼마나 달라질지는
이번 실행에서 분리해 측정하지 않았다.

기존 경로가 보고한 원래 geometry family의 크기는 **29,856,840 기하 후보**다.
이를 물리 해법 수, 원본 큐 수 또는 실제 방문한 후보 수로 바꾸어 해석하지 않는다.
해당 marker의 `completed = 0`도 이후 후보 방문이 없었다는 뜻이 아니다.

### 4.2 생성 이후에는 두 번째 rank의 Down 관계가 커졌다

첫 rank 완료 이벤트는 106.048초의 1초 표본에서 처음 확인된다. 이는 정확한 완료
시각이 아니라 첫 관측 시각이다. 마지막 문맥은 `rank = 2`, `piece = S`,
`row = 263 / 764`, `phase = row_down`이다. 기하 준비 뒤 약 28.5초 만에 종료됐다.

| 경과 시각 | 프로세스 RSS |
| --- | ---: |
| 125.055610초 | 1,709,023,232 bytes |
| 126.055909초 | 2,030,997,504 bytes |
| 127.056266초 | 2,353,197,056 bytes |
| 128.056470초 | 2,787,586,048 bytes |

이 네 표본에서 phase, rank, piece, row와 아래 work pulse 값은 같았다.

| Owner | 마지막 누적 work 표본 |
| --- | ---: |
| Boolean | 18,874,368 |
| Bridge | 19,660,800 |
| Down | 137,625,600 |
| ZDD | 137,363,456 |

work pulse는 262,144회마다 갱신하므로 매 순간의 정확한 작업 수가 아니다.
Down의 step 일부는 ZDD checkpoint도 증가시킨다. 두 값을 더한 것을 중복 없는
총 연산량이라고 부르지 않는다.

[`GeometryDownView`](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_partial_family.rs)의
필터는 `(선택 ZDD node, geometry derivative)` 조합을 처리한다. 호출별 필터 memo,
rank를 넘어 유지하는 derivative memo, 생성한 expression, ZDD 저장소가 존재한다.
`HashMap`/`Vec` 확장·rehash·복사·페이지 실체화는 work tick 사이에 발생할 수 있다.
고정된 work 표본과 빠른 RSS 증가는 큰 저장소 확장 중 순간 추가 메모리와 부합한다.
**정확히 어느 할당이 원인이었는지는 현재 로그만으로 구별되지 않는다.**

마지막 phase는 이동 fixed-point나 compaction이 아니다. 전체 파이프라인에는
compaction의 old/new 저장소 공존 위험도 있지만 이를 이번 마지막 phase의 원인으로
단정하지 않는다. 기존 경로는 준비 뒤 세부 phase가 없어 Build, coverage, memo,
결과 축적 중 어느 부분의 메모리인지 더 좁힐 수 없었다.

## 5. 정확성 확인 범위

이번 Full4 실행에서 비교 완료한 identity 수와 원본 source 수는 **각각 0**이다.
해법이 0개라는 결과가 아니라, 전체 결과 반환 전에 종료돼 검산 구간에 도달하지
못했다는 뜻이다. global coverage와 `(identity, source)` 전체 관계도 일치 판정을
내리지 않았다.

완료 결과가 생기면 하네스는 다음 검산을 수행한다.

1. 기존 결과의 count·packing·probability 완전성과 비절단 상태를 확인한다.
2. 새 결과의 전체 identity page를 추출해 typed/canonical 정렬·중복·개수를 확인한다.
3. 원본 4,233,600개 큐를 한 번씩 `covers_source()`로 질의한다. 전체 count와 같다는
   이유만으로 bitmap을 채우는 shortcut은 사용하지 않는다.
4. membership bitmap의 popcount와 보고된 exact covered-source count를 맞춘다.
5. 두 전체 identity 파일과 global bitmap을 EOF까지 직접 byte 비교한다.

검산은 탐색 반환 이후 별도 시간으로 기록한다. bitmap은 66,150개의 `u64`,
529,200 bytes이며 dense identity×source 행렬을 만들지 않는다. 성공하더라도 검증
범위는 두 projection이다. `joint_comparison_complete`는 false로 남기며 해법별 모든
큐 집합의 일치까지 주장하지 않는다.

실제 미완료 두 summary를 `--compare`에 넣었을 때
`a run lacks complete or internally consistent projection evidence`로 거절했다.
미완료 결과를 성공으로 통과시키지 않는 경계는 확인됐다.

로컬 correctness 검사는 ZDD/Down 15개, symbolic 통합 4개가 통과했다. 새 membership
검사 2개와 기존 fixture의 assertion 보강을 포함한다. 누락 변수=false, terminal,
잘못된 변수 순서, 깊은 경로, 숨겨진 원본 suffix, 초기 Hold, 공집합, 모든 builtin
profile의 기존 유한 fixture를 확인했다. 작은 입력의 성능 측정은 하지 않았다.
이 19개 통과를 Full4 전체 정확성 검증으로 대신하지 않는다.

과거 Jstris 기록의 456,459 identities와 `cts1:2fcbc950acdffa31`는 이전 결과다.
현재 작업공간에는 과거 전체 CTK1/coverage 본체가 없었고, 이번에는 새 완료 결과도
없으므로 실제 집합 비교에 재사용하지 않았다. SRS+의 다른 값과 섞거나 알려진
확률 명제를 추가로 탐색·증명하지 않았다.

## 6. 변경과 검증 기록

| 변경 | 목적 |
| --- | --- |
| `examples/symbolic_family_compare.rs`와 example 선언 | 같은 입력·바이너리, 분리된 시간, 완료 후 전체 projection 검산 |
| `symbolic_family.rs` | phase/context 관측, 원본 큐 길이와 읽기 전용 `covers_source` |
| `geometry_partial_family.rs` | 할당·계수·cache 변경 없는 ZDD membership, 낮은 빈도의 work pulse |
| `symbolic_boolean.rs`, `symbolic_family_bridge.rs` | 기존 work 증가 뒤 선택적인 pulse |
| 기존 `examples/symbolic_family.rs` | 신규 세부 marker를 출력 전 제외해 로그 폭증 방지 |
| 관련 두 테스트 파일 | membership 및 source identity 회귀 assertion |

코어의 pulse는 시계·문자열 formatting·파일·RSS 조회를 수행하지 않는다. 별도
샘플러가 1초마다 atomic 문맥과 RSS/HWM을 읽는다. geometry 완료 경계에서만
callback이 시각을 한 번 읽는다. 기본 제품 dispatch와 시간 한도, work 증가식,
명시적 한도·취소 판정은 바꾸지 않았다.

| 실행 | 결과 | 원시 기록 이름, `_local/artifacts/` 아래 |
| --- | --- | --- |
| ZDD/Down focused tests | 15/15 통과 | `full4-symbolic-membership-tests-20261123.log` |
| Symbolic integration tests | 4/4 통과 | `full4-symbolic-membership-integration-20261123.log` |
| release library build | 통과 | `full4-symbolic-release-library-20261123.log` |
| 첫 example build | JSON macro의 배열 반복 문법 오류 | `full4-symbolic-comparison-build-20261123.log` |
| JSON 필드 수정 후 동일 설정 build | 통과 | `full4-symbolic-comparison-build-fixed-20261123.log` |
| Legacy Full4 첫 실행 | SIGKILL, 미완료 | `full4-symbolic-legacy-20261123.log` 및 `.summary.json` |
| Symbolic Full4 첫 실행 | SIGKILL, 미완료 | `full4-symbolic-direct-20261123.log` 및 `.summary.json` |
| 미완료 증거 compare | 의도한 거절, exit 1 | `full4-symbolic-incomplete-comparison-20261123.log` |
| Rust 관측 집계 | 입력·바이너리 일치, 두 미완료 상태 확인 | `full4-symbolic-comparison-observation-20261123.json` |

Legacy receipt ID는 `1795421497885518335-3`, symbolic은
`1795421548085108837-3`이다. runtime JSON 사본도 각 실행 prefix의 `.runtime.json`으로
보존했다. 로그와 receipt 해시는 관측 JSON에 기록했다. 원시 실행 출력은 이 문서에
그대로 넣지 않고 검토한 수치와 해석만 선별했다.

CI 실행·조회·대기, Full6 탐색, OOM 이후 자원 변경 재시도는 하지 않았다.

## 7. 다음 개선의 우선순위

우선 원본 family의 기존 재사용·분해 구조를 살리고, 그 표현을 그대로 전달한 상태에서
Down의 실제 생존 관계와 할당 비용을 구분해야 한다. 이번 종료 위치는 rank 2이므로
마지막 결과의 pair count 계산만 생략하는 것으로 해당 구간을 해결할 수는 없다.

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs`, 구현 내용: fallible allocation과 명시적 resource 계약을 지키면서 무제한 symbolic 호출도 기존 residual memo와 component composition을 재사용하고, 분해된 DAG를 후보 전개 없이 직접 인계한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_partial_family.rs`, 구현 내용: derivative·호출별 filter memo·ZDD별 생존 크기와 확장 전후 capacity를 구분하고, 관련 없는 분기를 재작성하지 않는 공유 및 실제 재사용 수명에 맞춘 저장소 회수를 설계한다.
- [ ] 수정 파일: `crates/clearra-core-executor/examples/symbolic_family_compare.rs`, 구현 내용: 변경된 구조의 후속 평가에서 완료 결과를 확보한 뒤 전체 identity와 global 원본 큐 집합을 비교하고, 필요하면 별도 압축된 해법별 큐 관계 검산을 연결한다.

이 개선들은 특정 P7P4 큐나 SRS 계열 kick에 특화된 빠른 경로를 추가하는 작업이
아니다. 공유 관계의 크기와 확장 비용을 낮추는 것이 이번 관측으로 드러난 우선
과제다. 새 경로의 제품 전환은 아직 검증되지 않았다.
