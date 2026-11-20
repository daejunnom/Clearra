# Clearra: Rust 코어 통합과 의존성 기반 탐색의 실행 계획

작성일: 2026-11-20 KST

초안 조사 기준: `prototype/placement-dependency-20261120`, `caacfae0c9958949858bb846f2bb52eb5fcbb50d`

내부 연산 추가 감사 기준: `45cbf15eec6a6c8821946756807f90703466dff6`. 2.4절의 파일·줄 수는 초안 기준 집계이며 추가 감사의 새 집계가 아니다.

상태: **실행 계획과 소스 감사 문서. 추가 감사에서는 제품 코드 수정·빌드·벤치마크를 하지 않았다.** 병행하는 CEGIS 구현·검증의 진행과 결과는 5.7절의 별도 실행보고에 연결한다. 이 문서의 보강만으로 11절 작업을 완료 처리하지 않는다.

## 1. 결정과 우선순위

이번 작업의 중심은 **정확한 배치 관계를 만드는 데 필요한 계산 자체를 줄이고, 그 결과를 ILC → 타일링 → 조건·확률 계산까지 재사용하는 것**이다. 도달성 호출 한 번의 상수 비용을 줄이는 것만으로 성공을 판정하지 않는다. 관계를 만들기 위해 기존 BuildUp과 비슷한 수의 배치 순서를 다시 탐색한다면 목표를 달성하지 못한 것이다.

그 위에 프로젝트 소유 탐색 코어를 Rust로 통합한다. 이미 구현된 Rust CPU 탐색을 공통 엔진으로 발전시키며, 기존 Rust 구현을 조사하지 않은 채 C를 다시 옮긴 세 번째 엔진을 만들지 않는다. 요청·프로필·카탈로그·워커·결과의 실제 소유권을 명확히 하여 중복 생성과 불필요한 장기 보유를 줄인다.

우선순위는 다음과 같다.

1. 정확성 계약, 실행 경로, 공유 객체의 소유자를 고정한다.
2. 모든 미노에 적용하는 support OR·줄 삭제 사건·무관한 관계의 조기 제외부터 구현한다.
3. 기존 ILC·APDP·family의 반복 계산을 재사용·증분 전파로 줄인다.
4. 같은 Rust 코어를 native/WASM/제품 서비스에서 사용하도록 기능 격차를 닫는다.
5. 완전한 관계와 공급을 결합하여 개별 큐·물리 배치 순서 열거가 필요 없는 요청의 비용을 줄인다.
6. 증거가 확보된 기능부터 C 구현과 중복 wrapper를 퇴역시키고, 책임별 모듈 구조를 정리한다.

2·3·4는 공통 계약이 정해진 뒤 병렬로 진행할 수 있다. **C 전환이 끝날 때까지 관계 알고리즘 개선을 미루지 않는다.** 대규모 파일 이동도 알고리즘 작업 전체의 선행조건으로 만들지 않는다.

사용자의 Sfinder 9→10미노 설명은 실용적인 탐색 비용 급증에 관한 관측으로 이해한다. 불가능성 정리로 해석하지 않는다. 이 계획은 다른 도구와의 무조건적인 성능 우위를 주장하지 않으며, Clearra 내부의 동일 결과 계약에서 병목을 줄이는 것을 목표로 한다.

## 2. 조사에서 확인한 현재 상태

### 2.1 C와 Rust의 실구현이 함께 존재한다

`core-c/cmake/source_manifest.cmake`는 board, rule, supply, ILC/geometry, APDP, packing, reachability, BuildUp, coverage, memory를 실제 C 빌드에 연결한다. Git 추적 C 소스는 183개, 헤더는 102개로 합계 285개이며 모두 `core-c/` 아래에 있다. 작은 잔여 wrapper 몇 개만 옮기는 작업은 아니다.

동시에 `clearra-core-executor/src/backend/wasm_cpu/`에는 Rust의 geometry/ILC/APDP/BuildUp/reachability/coverage/확률 경로가 있다. `WasmCpuSearchBackend`라는 이름과 달리 native에서도 실행된다. `AppCoreExecutorBackend`에는 `NativeCore`와 `WasmCpu`가 따로 있으며, CLI의 `wasm-cpu-runtime` feature에 따라 선택도 달라진다.

두 경로의 기능은 아직 동등하지 않다. 예를 들어 `app_services.rs`의 NativeCore는 execution constraints 및 supplied-solution identity filter를 거절하는 분기가 있고, Rust 서비스의 기존 `execute_build_coverage_with_control` 진입점에는 미연결 오류가 있다. 따라서 backend 이름이나 enum 분기만 바꾸어 전환을 완료할 수 없다. 제품 요청별 실제 지원 범위를 먼저 확정한다. [S1]

### 2.2 이미 있는 구조를 새로 만들지 않는다

| 현재 기반 | 실제 소유자·심볼 | 계획에서의 역할 |
| --- | --- | --- |
| 미노 형상·프로필 | `clearra-piece-registry`, `clearra-rules` | 형상·ordered kick의 유일한 의미 원천 |
| ILC realization·skeleton | `catalog.rs::GeometryCatalog` | 구체 배치 ID와 realization의 공통 소유자 |
| 행 좌표 변환 | `row_frame.rs::CompactedRowFrame` | 기존 원래 행↔물리 행 대응 재사용; 제한된 wrapper 계약 보존 |
| 투영 선행 필터 | `inverse_projection.rs::ProjectionRowFilter` | 기존 필터 재사용; 같은 최적화를 새로 만들지 않음 |
| 삭제 행별 실현 | `compile_instantiation_table`, `instantiations` | 정확한 frame-domain의 권위로 정리 |
| cell→placement incidence | `GeometryCatalog::support` | 무관한 관계를 열거 전에 제외하는 출발점 |
| 정확한 APDP 부모 | `geometry_apdp.rs::ExactArmPairIndex` | partial별 full parent 대안 보존 |
| residual 재사용 | `geometry.rs::ResidualMemo` | 같은 잔여 기하 문제의 기존 재사용 유지 |
| completion 재사용 | `geometry.rs::GeometryCompletionOracle` | before/after-clear compiler와 available-row cache 재사용 |
| 타일링 family | `geometry_family.rs::GeometrySolutionFamily` | `Append/Union/Product` 및 공유 suffix 활용 |
| projection·component | `geometry_projection.rs`, `geometry_component.rs`, `geometry_separator.rs` | 기존 기하학적 증명 재사용; 이동 독립성과 혼동하지 않음 |
| 공급·coverage | `clearra-supply`, `clearra-coverage` 및 기존 확률 경로 | hold·가중치·큐 합집합 의미 유지 |

확인한 중복은 다음과 같다.

- `PlacementConditionCatalog`는 카탈로그의 모든 삭제 행 상태를 다시 조회하여 정확한 domain을 만들고 cell incidence를 복제한다. 소비자에게 필요한 view를 제공하는 단일 소유자가 후보이다.
- `normalized_kick_delta`와 `normalized_rotation_center`가 일반 reachability, extended reachability, finesse에 각각 존재한다. 의미가 같은 이 정의부터 공통화할 수 있다.
- C FFI의 `CoreContext = ContractCoreContext`와 실제 C 메모리를 감싼 `NativeCoreContext`가 공존한다. 이름이 아닌 실제 호출 경로와 수명을 기준으로 책임을 정리한다. [S2][S3]

`clearra-exact-cover`의 현재 `solve_first` 중심 solver와 ILC 전체 family compiler는 계약이 다르다. `clearra-geometry`의 정적 placement 열거도 stretched target-frame ILC를 자동 대체하지 않는다. **기존 코어를 우선 검토하되, 의미가 같은 부분만 재사용한다.**

`CompactedRowFrame`은 기존 compact 행 대응의 소유자이고 `LocalRelationRowFrame`은 이를 감싸 더 좁은 범위를 허용한다. 이 wrapper를 중복으로 지우지 않는다. 다만 `catalog.rs`와 `buildup.rs`에도 행 사상이 있으며 저장소 전체의 행 변환이 이미 한 구현에 통합된 상태는 아니다. `CompactedRowFrame`은 높이 1~6·`u8` 삭제 mask이고 일부 보드 adapter는 폭 10을 전제한다. C target-frame 투영의 최대 16행·`u16` mask 계약을 이 좁은 객체로 대체하지 않는다. 순수 행사상과 보드 adapter를 구별하여 범위별 일반화 가능성을 검증한다. `ProjectionCatalog`가 compact/extended 입력을 `compile_projected`로 합치는 것은 기존 재사용 사례다. [S10][S12]

### 2.3 APDP 적용 범위와 메모리 문제를 구분한다

현재 `FamilyCompiler::advance`에서 고급 domain 전파는 `!resource_authoritative`, 목표 깊이 7 이상, 비어 있지 않은 초기 필드라는 조건을 갖는다. 빈 필드 P7P4는 `compile_minimum`을 사용한다. APDP 자체의 개선을 빈 P7P4 개선이라고 보고하지 않는다. 활성 범위를 넓히는 작업은 정확성·자원 계약·손익을 검증하는 별도 과제다. [S4]

최근 P7P4 미완료 실행 세 번은 Rust 백엔드의 native 실행에서 발생했다. 종료 137/SIGKILL, 빈 결과 JSON, 표본 RSS 최대 약 4.64~4.74 GB라는 기록은 있으나, 커널 OOM 또는 외부 종료 중 어느 쪽인지는 확정하지 못했다. `runtime.rs`의 `status.code().unwrap_or(125)`가 Unix 종료 신호를 잃는 진단 문제는 소스에서 확인했다. **Rust 전환을 이 종료 문제의 검증된 해결책으로 취급하지 않는다.** [S5]

### 2.4 파일 수는 소스와 산출물을 나눠 관리한다

조사 기준 Git 추적 파일은 **4,211개**, `.rs` 파일은 **2,403개**, `tools/vendor/`를 제외한 자체 Rust 파일은 **2,264개**다. 사용자의 다른 작업환경에서 수만 개가 보이는 상황과 이 스냅샷의 수치를 동일시하지 않는다. 빌드·의존성 산출물의 수는 소스 모듈 수와 별도로 집계한다.

파일 수 감소의 우선 수단은 중복 엔진·중복 adapter·불필요한 호환 shim의 퇴역과 응집도 회복이다. 파일을 합쳐 숫자만 줄이거나 하나의 타입마다 새 파일·새 crate를 만드는 것을 목표로 삼지 않는다. 상세 기준은 10절에 둔다.

| Git 추적 경로 | 파일 수 |
| --- | ---: |
| `crates/` | 2,287 |
| `scripts/` | 561 |
| `core-c/` | 293 |
| `packages/` | 273 |
| `apps/` | 244 |
| `tools/` | 193 |
| `tests/` | 162 |
| `docs/` | 142 |
| 나머지 설정·자산·루트 파일 | 56 |
| 합계 | 4,211 |

자체 Rust에서 `tests/`, `benches/`, `examples/`, `_tests.rs`, `tests.rs`를 제외한 계획용 분류는 1,862개 파일이다. 이 중 100줄 이하 981개, 1,000줄 초과 118개이며, 인라인 테스트는 포함된다. 이 수치는 과분할·SRP 위반의 판정이 아니라 조사 우선순위다. `mod.rs` 같은 정상적인 작은 연결 모듈도 포함된다.

큰 파일의 예는 `exact_minimum_cover.rs` 12,238줄, `build_probability.rs` 8,248줄, `desktop_request_bridge.rs` 7,608줄, `result.rs` 7,376줄, `buildup.rs` 5,338줄이다. 줄 수는 빈 줄·주석·인라인 테스트를 포함한다. 반면 `clearra-app`의 `request.rs`·`response.rs`는 각각 4줄, `run_request.rs`는 5줄이다. 양쪽 모두 실제 책임과 호환성부터 확인한다. [S11]

## 3. 모든 작업자가 지켜야 하는 계약

아래 내용은 작업 배정과 다른 세션 인계 시 그대로 전달한다. 후속 담당자가 일부 성능 항목만 읽고 방향을 바꾸지 않도록 각 작업의 완료 조건에도 적용한다.

1. **ILC·APDP에서는 반복 연산을 제거하는 재사용·증분 처리·알고리즘 개선만 다룬다.** 자료구조 패킹, 비트폭, padding, 해시 컨테이너 교체, cache-line, SIMD, 작은 할당 상수 최적화는 이 트랙의 검토·성과 범위 밖이다. 35비트 전용 공급 표현도 우선 과제에 포함하지 않는다.
2. 관계 구성 비용과 구성 중 physical reachability 호출 수를 먼저 측정한다. BFS를 전처리·predicate 내부로 옮겨 비용이 사라진 것처럼 보고하지 않는다.
3. 동일 kind의 서로 다른 구체 placement/parent는 다른 ID다. 선택 여부, 실행 여부, 현재/hold 공급 발생은 서로 다른 의미다.
4. 조건의 OR, 부정, parent 대안과 frame의 상관관계를 보존한다. 단순 DAG 또는 선행자 집합 교집합으로 모든 관계를 대신하지 않는다.
5. kick first-success 순서, 동시 줄 삭제, 원래 행과 물리 행의 좌표 계약을 보존한다. 최대 kick 거리만으로 전체 영향 반경을 단정하지 않는다.
6. 필요한 조건만 확인한 결과를 충분조건으로 쓰지 않는다. Unknown·미지원·자원 부족은 Impossible이 아니다. 불완전 실행을 완전한 확률로 출력하지 않는다.
7. 새 객체·crate·캐시를 만들기 전에 기존 소유자와 재사용하지 못하는 구체적인 이유를 제시한다. digest 일치는 의미 동등성 증명이 아니다.
8. 생산 Rust 코어는 하나로 수렴한다. 검증용 독립 oracle과 명시적인 compact/extended 특수화는 유지할 수 있다.
9. CountUnique, CountAll, hold 분기 다중성, 큐 universe·가중치, 해별/전체 coverage, terminal evidence와 completeness를 각각 검증한다.
10. 워커는 환경의 자동 논리 프로세서 수를 유지한다. 자원 정책이나 워커 수를 줄여 실패를 숨기지 않는다. 큰 벤치마크는 manager 아래에서 순차 실행한다.
11. 원시 실행 기록은 `_local/artifacts/`, 검토한 해석은 `docs/research/`에 둔다. 코드 이동·성능 변경·정책 변경을 한 변경에 섞지 않는다.
12. 이번 문서는 구현 완료 기록이 아니다. 아래 체크리스트는 후속 구현과 검증을 통과해야 완료 처리한다.
13. **SRP는 작업·리뷰 규약으로만 사용한다.** 1,000줄 제한, 필수 `SRP rationale:` 문구, marker 존재 여부, 파일·타입·crate 개수로 합격을 판정하는 정적 gate를 새로 만들거나 복구하지 않는다. 정해진 문구나 주석의 존재는 구현·증명의 근거가 아니다.
14. Rust에서는 속도와 메모리 효율성을 위해 통상적인 SRP 경계를 넘어서 연산·상태를 함께 둘 수 있다. 분리가 재계산·복사·할당·장기 보유·dispatch를 늘린다면 융합을 검토한다. 변경 리뷰에 실제 연산·소유권의 이유를 남기되 파일별 의무 문구나 예외 승인 제도를 만들지 않는다. 정확성·완전성·실제 의존성·ABI·unsafe 경계 계약은 그대로 지킨다.

## 4. 목표 구조와 객체의 책임

객체화의 기준은 **의미와 불변식을 명시하고, 실제 연산과 데이터 수명을 관리하는 것**이다. Rust에 클래스 계층을 강제하거나 책임 하나마다 객체·trait를 추가하지 않는다. 한 커널이 여러 역할을 함께 수행하는 편이 반복 연산과 메모리를 줄이면 그 구조를 유지할 수 있다. 다음은 계약을 논의하기 위한 역할이며 각 행을 새 crate나 새 파일로 만들라는 요구가 아니다.

| 역할 | 기존 소유자 우선 | 소유해야 할 내용 | 소유하지 않을 내용 |
| --- | --- | --- | --- |
| 규칙·형상 정의 | `clearra-rules`, `clearra-piece-registry` | ordered kick, 정규화 좌표, capability | 후보별 배치 상태 |
| 요청 계약 | `clearra-problem`, `clearra-core-domain` | 보드·목표·공급·ID·완료 상태 | ABI 포인터, UI 상태 |
| 불변 기하 카탈로그 | 기존 Rust `GeometryCatalog` | realization, exact frame-domain, incidence, APDP parent | queue cursor, hold, 장기 누적 결과 |
| 타일링 family | 기존 `GeometrySolutionFamily` | 기하학적 대안과 suffix 공유 | 증명 없는 이동 독립성 |
| 관계 compiler | 기존 placement/structural 모듈 정리 | support·movement·clear·terminal-label 조건 | 숨은 순차 BuildUp |
| 공급·목표 결합 | 기존 supply/language/coverage 경로 | 큐 합집합, 가중치, hold, 목표 상태 전이 | 새로운 물리 도달성 구현 |
| 워커 workspace | 기존 BuildUp/reachability workspace | 해당 워커의 가변 scratch와 짧은 binding | 요청 전체 불변 데이터의 깊은 복제 |
| 결과·증거 | 기존 result/replay/coverage 객체 | 요청한 결과와 필요한 재생 증거 | 사용이 끝난 전체 중간 그래프 |
| 제품 orchestration | `clearra-app` | 계약 검증과 서비스 연결 | 두 번째 solver |
| 호스트 adapter | CLI, Web, Discord, desktop | 수송·스케줄링·표시 | 별도 미노/확률 규칙 |
| ABI adapter | `clearra-core-ffi`의 축소된 경계 | 확인된 외부 호환 계약 | 도메인의 원천 정의 |

`SearchSession`과 같은 수명 소유자는 기존 요청 객체에 대응시키는 설계 이름으로 사용한다. API를 새로 만들기 전에 기존 session/workspace가 같은 책임을 이미 갖는지 확인한다. 불변 카탈로그는 요청 단위로 공유하고, worker scratch는 worker가 소유하며, candidate relation binding은 소비가 끝나면 해제한다.

동일 shape와 rule 의미는 함께 쓴다. Board64와 extended board의 실행 커널은 능력·비용 차이가 있으면 특수화를 남긴다. 모든 것을 하나의 거대한 generic 함수로 합치는 것도, 호스트마다 전체 알고리즘을 복제하는 것도 피한다.

## 5. 관계 알고리즘: 무엇을 미리 만들고 무엇을 생략할 것인가

### 5.1 관계의 기본 단위

타일링 선택 변수 `s_p`와 이미 배치한 변수 `x_p`를 구분한다. 삭제된 원래 행 집합을 `D`, ordered kick 프로필을 `K`라고 할 때 목표 배치 `q`의 조건을 다음처럼 분해한다.

\[
Enabled_q = \neg x_q \land Selected_q \land Frame_q(D)
             \land Support_q(X,D) \land Entry_q(X,D,K).
\]

위 식은 최종 데이터구조를 미리 고정한 것이 아니다. 각 인자가 무엇을 증명하는지 분리하여 같은 의미를 재계산하지 않기 위한 계약이다. 지원하는 범위에서 완성된 관계의 실행 언어는 물리 실행 언어와 양방향으로 같아야 한다.

\[
\mathcal L_{relation}=\mathcal L_{physical}.
\]

순수 AND 선행조건만 남는 경우에는 기존 poset 경로를 재사용한다. 일반적인 OR·부정·조건부 clear 사건에는 조건식 또는 사건 hypergraph가 필요하다. 표현을 바꾸었다고 최악의 경우의 지수적 복잡도가 없어지는 것은 아니다. 공통 부분, 독립 부분, 반복 경계의 계산을 제거하는 것이 실질적인 목표다.

### 5.2 바닥과 supporter OR를 가장 먼저 처리한다

현재 물리 frame에서 목표 셀 집합을 `P_q^D`라 하면 아래쪽 외곽 `F_q^D`가 받침 검사 대상이다.

\[
F_q^D=Down(P_q^D)\setminus P_q^D,
\quad Support_q=floor\lor initialSupport\lor\bigvee_p(x_p\land Supports(p,q,D)).
\]

Z가 L 또는 S 위에서 멈출 수 있다면 조건은 `x_L OR x_S`다. 이를 L과 S 모두 필요하다는 두 개의 필수 선행 간선으로 바꾸지 않는다. 현재 BuildUp에도 grounding 검사가 있으므로 중복 검사를 추가하는 것이 핵심이 아니다. **타일링 이전에 supporter 대안을 만들고, 관계 구성·부분 타일링에서 재사용**하는 것이 변경점이다.

부분 타일링에서는 아직 선택할 수 있는 호환 parent와 허용 frame을 고려한다. 현재 선택한 조각 중 받침이 없다는 이유만으로 prune하지 않는다. 모든 미래 대안에서도 support가 불가능할 때만 제거한다.

### 5.3 무관한 관계를 생성하지 않는다

먼저 모든 미노 쌍을 만들고 값비싼 검사로 지우는 방식보다, 기존 cell incidence를 통해 증명에 필요한 셀의 owner만 가져온다.

1. 규칙·목표·frame에 유효한 영향 범위를 얻는다.
2. 그 범위와 만나는 concrete placement 후보만 찾는다.
3. 변하지 않는 점유, 이미 결정된 분기, 닫힌 분리 경계로 불필요한 변수를 제거한다.
4. 여전히 영향을 줄 수 있는 관계만 조건식에 남긴다.

범위가 겹친다는 것은 관계 후보라는 뜻이다. 실제 의존성이 있다는 증명과 다르다. 반대로 단독 A/B 비교만으로 보편적인 무관함을 증명할 수도 없다. A와 B가 서로 다른 진입로를 각각 막아 함께 있을 때만 실패하는 반례를 포함한다.

O의 행 단조 계산은 검증된 특수 규칙으로 유지한다. 다른 미노에는 ordered kick 분기 조건을 적용한다. j번째 kick이 선택되려면 다음 조건이 필요하다.

\[
Chosen_j=Free(t_j)\land\bigwedge_{i<j}\neg Free(t_i).
\]

장애물 제거가 앞선 kick을 성공시켜 원하는 뒤쪽 kick을 선택하지 못하게 할 수 있다. 따라서 O의 단조성을 모든 미노로 확장하지 않는다. 하드드롭·열린 진입 경로·불변 kick prefix·닫힌 통로 분리·증명된 이동 성분 등을 독립 규칙으로 일반화한다. 하드드롭 성공은 충분조건이지만 실패는 전체 배치 불가능의 증명이 아니다.

현재의 정적 influence mask는 보조 수단이다. 이것을 만드는 pose topology 순회까지 후보마다 반복한다면 주목표에 어긋난다. 규칙 형태를 공유하고, 해당 범위가 전체 필드가 되는 어려운 사례는 정직하게 남긴다.

### 5.4 줄 삭제는 별도 사건이지만 자동으로 발생한다

원래 행 r의 빈칸을 채우는 concrete contributors를 `C_r`라 하면, 초기 완성 행 처리 계약을 정규화한 뒤 행 완성 조건은 `AND_{p in C_r} x_p`로 표현한다. O가 S의 이동을 직접 막지 않고 줄 삭제를 통해서만 영향을 준다면 다음 관계로 보존한다.

`O 및 다른 contributors → 행 완성 → 동시 삭제 사건 ΔD → S의 새 frame 조건`.

줄 삭제는 플레이어가 임의로 선택하거나 지연하는 사건이 아니다. lock 직후 완성된 모든 행을 한 사건으로 삭제한다. 지워진 미노의 일부 셀은 남아 충돌에 영향을 줄 수 있으므로, 이미 실행된 미노에 pending placement의 ILC domain을 잘못 적용하지 않는다.

tetromino 하나가 새로 완성할 수 있는 원래 행은 최대 네 개다. 이는 국소 사건 분류의 작은 상한이며, 전체 관계 상태가 16개뿐이라는 주장이 아니다. 행 contributor와 이동 영향 관계를 분리하면 직접 영향이 없는 미노를 이동 조건에서 제외하면서도 간접 효과는 정확하게 남길 수 있다.

### 5.5 B2B와 다른 목표 조건

기존 B2B-save 정책은 0줄, 4줄, PC, 또는 인정되는 spin을 허용한다. 숫자 B2B chain의 갱신 규칙은 별개다. PC 예외를 chain 증가 규칙으로 혼동하지 않는다. [S6]

관계는 배치 가능성뿐 아니라 lock/clear의 최소 terminal evidence를 낸다. 목표별 상태 기계가 그 evidence를 소비하도록 한다. B2B-save만 요청한 0줄·4줄·PC 전이에서 불필요한 spin evidence를 만들지 않는 기회를 검토한다. 1~3줄·non-PC에서 T-only 프로필의 non-T 전이를 조기에 제외하는 것은 해당 정책의 정확한 적용 범위 안에서만 허용한다.

`core-executor → scoring`의 직접 runtime 의존성을 새로 추가하지 않는다. 기존 정책을 복사하지 않고, 제품 계층이 준비한 중립적인 goal/event 계약을 relation evaluator에 전달하는 방향으로 경계를 설계한다. 현재 runtime에서 `clearra_postprocess::BackToBackEdgePolicy`를 거치는 경로도 조사·정리 대상이다. 순수 목표 정책과 물리 terminal evidence를 분리하되 정책의 의미 권위는 하나로 유지한다.

CountAll이 요구하지 않는 한 동일 배치의 여러 terminal 증거가 결과 수를 늘리면 안 된다. 순서 교환도 물리 관계, 삭제 사건, 목표 상태, 공급 계약을 모두 보존할 때만 허용한다.

### 5.6 수동 CEGIS 방식의 발전

반례 기록은 두 종류로 구분한다. 첫째는 실제 허용/불허·확률이 잘못된 의미 반례, 둘째는 정답이지만 관계 완성에 탐색이 많이 필요한 비용 반례다.

각 사례에서 관련 placement, row 사건, kick prefix를 최소화한다. 그다음 여러 사례를 포괄하는 규칙을 만들고 적용 범위를 증명한다. 기존 작은 물리 oracle과 제품 계약으로 검증한 뒤, residual 검색이 얼마나 제거됐는지 측정한다. 유한 테스트에서 반례가 없었다는 사실을 모든 필드의 증명으로 승격하지 않는다.

개발 중에는 기존 정확 BuildUp을 비교·fallback으로 사용할 수 있다. 하지만 fallback이 남은 비율과 비용을 공개해야 하며, 모든 대상 관계가 완성됐다고 선언할 때는 predicate 내부에 물리 탐색을 숨길 수 없다. 일반화가 아직 안 된 범위는 범위별 미완성으로 남긴다.

### 5.7 병행 CEGIS 구현의 결과와 다음 우선순위

[Translation interval·내부 연산 재사용 실행보고](translation-interval-and-operation-reuse-2026-11-20.md)에 모든 표준 미노의 고정 방향 좌우·하강 양성 구간 증명, 적용 계약, 독립 검증과 제품 결과를 기록했다. 해당 증명은 BFS/pose 방문 집합 없이 구간 전체를 보장하지만, 측정한 비-O 제품 사례의 전체 graph/primitive 감소는 0이었다. 기본 비활성 실험으로 유지하며 큰 검색의 성능 향상이나 G06 전체 완료로 표시하지 않는다.

T/L 반례의 정확한 언어는 `{TL}`로 일반 poset가 표현할 수 있다. 빠진 것은 **이동과 줄 삭제로 인한 차단에서 필요한 선행 간선을 도출하는 단계**다. 현재 compiler는 `D=0`의 T 양성 증명을 얻고도 `D={0}`에서의 실패를 올바르게 Unknown으로 남겨 전체 인증을 중단한다. 양성 guard 하나를 전체 관계 완성으로 계산하지 않는다.

다음 작은 작업의 순서는 G01의 증거 권한·조건 계약, G05/G09의 모든 마지막 이동 역상을 포함하는 bounded 부정 증명, G03의 clear 원인과 해당 부정 조건 결합, G06/G15의 유효한 부분 증명 재사용이다. 역상 전체가 경계/최소 점유에 막힌 경우만 부정하며, 일부 이동이나 특정 kick만 검사한 실패를 간선으로 승격하지 않는다. 특정 frame의 차단을 무조건 선행관계로 일반화하려면 그 조건이 모든 관련 prefix에서 유지된다는 증명이 추가로 필요하다. 이 부분 계약이 준비되면 기존 typed relation과 supply를 연결하고, 큰 family 재설계 전체를 기다리지 않는다.

## 6. ILC·APDP의 구조적 최적화

### 6.1 ILC: 정확한 frame-domain을 한 번 정의한다

`required_deleted_rows`, occupied rows, skeleton requirement domains, 정확한 instantiation table, placement의 exact clear domain은 서로 관련되지만 같은 강도의 증거가 아니다. 필요한 삭제 행 조건만으로 정확한 실현 가능성을 대체하면 안 된다.

먼저 `RowFrame → RealizationDomain → PlacementDomain`의 의미를 한 소유자에서 정의하고 각 소비자에게 필요한 view를 준다. 기존 incidence도 복제 생성보다 소유자 참조를 우선한다. 지원 범위가 다르거나 생명주기가 달라 복사해야 하는 경우에는 이유와 보유 비용을 남긴다.

그다음 다음 후보를 검토한다.

- 같은 shape·row assignment·frame의 투영을 재계산하지 않는 공유 단위.
- 실제 소비되는 frame에 한정한 정확 instantiation과 이미 계산된 결과의 재사용.
- 필요한 삭제 행, 자기 행 삭제 금지, 정확 투영 조건을 합성한 기호적 domain. 높이에 따른 모든 D 열거를 줄일 가능성이 있으나 **필요충분성은 아직 증명 과제**다.
- before-clear와 after-clear compile domain을 구별한 증분 재투영.
- compact/extended가 같은 투영 의미를 공유하되 용량·지원 범위는 보존하는 공통화.

삭제 이후 영향받는 범위에는 삭제 행 주변뿐 아니라 그 위에서 이동한 surviving cells·미래 배치·support·kick 조건도 포함한다. 이전 frame의 무영향 증명은 새 frame에서도 전제가 보존되는지 확인했을 때만 재사용한다.

캐시를 추가하는 것 자체가 성과가 아니다. 새로 유도한 realization 수, exact-domain 평가 수, raw instantiation 수가 줄었는지 확인한다. 폭·높이·shape·profile·초기 필드·domain 정책 중 결과에 영향을 주는 요소를 공유 key에서 빠뜨리지 않는다.

### 6.2 APDP: 변화한 parent만 다시 전파한다

현재 residual memo는 동일 잔여 상태를 재사용한다. 새로운 후보는 **부모와 자식 residual 사이에서 바뀌지 않은 지원 도메인 검사를 생략**하는 것이다.

placement가 선택되면 겹치는 full parent가 제거된다. 공급 가능한 kind가 바뀌면 그 kind의 parent가 달라진다. 영향받은 cell/partial만 갱신하고, SameTile·common owner·APDP 증거를 정확히 무효화하여 다시 전파한다. 되돌리기는 해당 branch에서 바뀐 의미 상태를 복구한다. full compile 결과를 oracle로 하여 모든 분기의 살아 있는 parent 집합을 비교한다.

APDP parent 대안과 frame을 유지한 관계 조각은 다음 형태다.

\[
\bigvee_{p\in Parents(a)}
 (Selected_p\land FrameDomain_p\land RelationFragment_p).
\]

어떤 parent에서 L support가 가능하고 다른 parent에서 S support가 가능한 것을, L과 S가 동시에 존재하는 하나의 상태로 합치지 않는다. static-exact APDP 범위를 우선 사용하고 temporal APDP는 별도 연구·증명 이후 확대한다.

### 6.3 기존 family에 결합하되 prefix 의존성을 숨기지 않는다

기존 `Append`에는 새 placement가 바꾸는 관계만, `Union`에는 각 대안의 상관관계, `Product`에는 실제 component 경계 조건을 결합한다. 완성 타일링마다 관계를 처음부터 만들지 않는 방향이다.

다만 geometry의 residual key가 같아도 선택한 prefix의 concrete placements, row contributors, 진입 통로는 다를 수 있다. **family ID만으로 완성된 이동 관계를 재사용하지 않는다.** 공유 대상은 경계 owner/clear 조건을 매개변수로 받는 관계 조각이며, 이 인터페이스를 작게 유지할 수 있는지는 연구 과제다.

geometry component가 분리됐다고 movement·line clear·B2B·hold까지 독립인 것은 아니다. 각 독립성을 별도로 증명한 경우에만 결과를 합성한다. 관계 조각이 더 커져 전체 비용이 증가하면 첫 배포의 필수조건으로 고집하지 않는다.

## 7. 전면 Rust 전환과 메모리 수명

### 7.1 전환 순서

1. 제품 요청 × 호스트 × feature × 현재 backend × 결과 계약의 표를 동결한다.
2. 규칙 정규화, ID, row-frame, exactness 및 resource 결과의 의미 원천을 공통화한다.
3. 기존 Rust geometry와 verification을 공통 CPU 서비스로 노출한다. `wasm_cpu` 이름 변경은 기능 이전과 분리한다.
4. Rust에 없는 제품 연결을 기존 supply·coverage·replay 경로로 보완한다. 이미 존재하는 기능은 재구현하지 않는다.
5. 동일 요청의 C/Rust 결과 및 독립 oracle을 비교한다. 공통 지원 범위와 남은 격차를 구분한다.
6. 제품 adapter를 Rust 서비스로 바꾸고, C는 qualification 전용으로 격리한다.
7. 확인된 C ABI 소비자만 호환시키고, production의 C 링크·CMake 경로·불필요한 FFI를 퇴역시킨다.

C ABI는 구현 언어가 아니다. 외부 소비자가 필요하면 Rust가 얇은 ABI를 내보낼 수 있다. 현재 Rust→C import와 미래 Rust→외부 export를 혼동하여 같은 심볼을 이중 링크하지 않는다. 외부 소비자가 확인되지 않은 ABI 전체를 영구 재구현하는 일을 우선하지 않는다.

프로젝트 solver C가 전환 대상이다. 의존 라이브러리 내부의 C, 운영체제 API, GPU shader까지 모두 제거하는 별도 사업으로 확대하지 않는다. C oracle 퇴역 후에도 작은 독립 물리 oracle과 adversarial fixture는 남긴다.

### 7.2 메모리 개선은 생성량과 생존 기간을 줄인다

다음은 관측 항목을 분류하기 위한 모델이다.

\[
M_{peak}\approx M_{shared}+W M_{worker}+M_{live\ candidate}
                  +M_{retained\ result}+M_{transport}+M_{runtime}.
\]

실제 할당기 여유·스택·라이브러리 등을 포함해 RSS와 모델의 차이를 기록한다. 객체 추정 합계를 실제 peak RSS라고 표현하지 않는다.

- 불변 catalog/template를 요청에서 한 번 만들고 재사용한다.
- 워커 scratch는 워커 수명에 맞추고, 후보별 임시 관계는 소비 후 놓는다.
- 검증·점수·출력의 전체 그래프를 동시에 유지해야 하는지 결과 모드별로 검토한다.
- 확률만 필요한 요청에는 그 확률을 증명하는 요약을 남기고, 모든 trace 요청에는 필수 증거를 보존한다.
- streaming/batch 경계에서 이전 단계 소유권을 실제로 이전·해제한다. 강한 참조의 순환이나 전역 캐시가 이전 요청 전체를 붙잡지 않게 한다.
- 유지해야 할 정확한 결과가 예산을 넘으면 자원 불완전 상태를 보고한다. 결과 일부를 버리고 완전 확률로 표시하지 않는다.

현재 C의 arena/pool/scratch 이름은 실제 구현에서도 서로 다른 최적화라는 보장이 없다. 일부는 같은 malloc 기반 scope 함수로 연결되고, geometry/workspace에는 별도 malloc도 있다. 이 체계를 이름 그대로 Rust pseudo-GC로 복제하지 않는다. 실제 소유권과 해제 시점부터 설계한다. [S3]

진단 작업은 알고리즘과 독립적으로 먼저 진행할 수 있다. 정상 exit·Unix signal·manager 종료를 구분하고, 커널 peak와 표본 peak, 읽을 수 있는 외부 한도와 unavailable을 구분한다. hot path에 supervisor나 노드별 시계를 넣지 않는다.

### 7.3 전환·재사용의 실제 단위: 12개 내부 연산군

아래 OP01~OP12는 11절의 88개 작업을 구체화하는 감사 지도이며, 새 작업 수·파일 수·crate 수가 아니다. **이미 같은 계산이 있는지, 같은 이름이지만 명제·지원 범위가 다른지, 계산은 있으나 제품 연결이 없는지를 먼저 구분한다.** 경로가 길어지는 것을 피하기 위해 이 절의 `W/`는 `crates/clearra-core-executor/src/backend/wasm_cpu/`, `C/`는 `core-c/src/`를 뜻한다. 이름이 같은 helper를 합치는 것만으로 성능 개선이라고 판정하지 않는다.

#### OP01 — 행 사상과 줄 삭제 누적: O03·O07·I06

- **기존 구현·소비자:** C의 `packing/target_frame_projection.c::{clearra_target_frame_project_lock_operation,clearra_target_frame_merge_deleted_rows}`; Rust `crates/clearra-core-executor/src/row_frame.rs::CompactedRowFrame`, `W/catalog.rs::target_row_for_current_row`, `W/buildup.rs::{merge_deleted_rows,place_and_clear,compact_target_board}`. ILC·BuildUp·관계·replay가 소비한다.
- **입출력·불변식:** `(target height, 삭제된 원래 행 D, 물리 행)`에서 원래 행을 얻고, 새 물리 삭제 행은 이전 프레임으로 역변환한 후 D에 합친다. surviving rows의 순서를 보존한다. 삭제 행 개수만 같은 두 D를 동치로 보지 않는다.
- **공유·무효화:** 동일 owner의 `(height,D)` 행사상은 공유 가능하다. 보드 layout·target 높이 또는 D가 바뀌면 해당 변환의 유효성을 다시 판정한다. compact의 1~6행·`u8` mask/폭 10 adapter와 C의 최대 16행·`u16` mask를 동일 범위로 취급하지 않는다.
- **남은 작업:** 순수 행사상과 좁은 보드 adapter의 계약을 분리해 중복 계산을 연결한다. 좁힌 `LocalRelationRowFrame`은 별도 의미를 갖는 wrapper이므로 무조건 제거하지 않는다. [S10][S12]

#### OP02 — 형상 좌표와 ordered kick 정규화: O01·O02·O07·G05

- **기존 구현·소비자:** `clearra-piece-registry`, `clearra-rules/src/kicks`; C `reachability/kick_first_success.c::{clearra_reachability_kick_offsets_for_transition,clearra_kick_first_success}`; `W/reachability.rs`, `W/extended_reachability.rs`, `clearra-finesse/src/lib.rs`의 `normalized_kick_delta`·`normalized_rotation_center`. `W/kick_profiles.rs`는 이미 Rust `SrsKicks`·`NoKick`를 사용한다.
- **입출력·불변식:** `(shape definition, from/to rotation, 순서 있는 kick offsets, 좌표 규약)`을 정규화된 후보 전이로 만든다. delta는 `kick + center(from) - center(to)`이며 앞 후보 실패 조건과 선택된 kick index를 잃지 않는다. 같은 occupied mask도 orientation·spin/finesse evidence까지 같은 것은 아니다.
- **공유·무효화:** 불변 shape/rule/dimensions/source-entry 계약별 템플릿을 공유한다. builtin profile ID만으로 식별하는 계약을 custom profile까지 넓힐 때에는 실제 내용·버전의 동일성도 보존한다. shape registry·180 허용·좌표 또는 source-entry 규약 변경 시 기존 증명이 유효한지 확인한다.
- **남은 작업:** 세 Rust 정규화 소비자의 의미 원천을 통합하고 ABI ID 인코딩만 분리한다. 관계 compiler는 이 전이 계약을 재사용한다. 최대 kick offset만으로 독립적인 영향 반경을 새로 추정하지 않는다. [S2][S13]

#### OP03 — ILC 행 투영 필요조건: I05·I08

- **기존 구현·소비자:** `W/inverse_projection.rs::ProjectionRowFilter::compile`, compact·extended inverse catalog, C `packing/target_frame_projection.c`. Rust 필터는 local row별 열 점유와 허용 target row를 만들고 열별 요구량과 가용량의 모순을 제거한다.
- **입출력·불변식:** `(projection policy,width,height,shape cells,local rows,x,available-row 의미)`에서 허용 투영 후보를 얻는다. 이는 **필요조건 필터**이며 leaf의 정확 projection을 대체하지 않는다.
- **공유·무효화:** shape kind만이 아니라 rotation/x/열 점유와 available-row 조건이 같을 때 공유한다. residual 변경으로 가용 행 의미가 바뀌면 이전 필터를 그대로 사용하지 않는다. 기존 compact/extended 공용 필터를 또 만들지 않는다.
- **남은 작업:** 동일 열 요구식·행 가용 조건을 여러 ILC 분기에서 다시 생성하는 부분을 찾아 전달·증분 갱신한다. bitfield나 container 변경은 이 작업의 범위 밖이다. [S12]

#### OP04 — 정확 realization과 clear-state domain: I01~I03·O04

- **기존 구현·소비자:** C `packing/geometry_realization_domain.c::{clearra_geometry_catalog_instantiate_realization,clearra_geometry_catalog_skeleton_supports_clear_state,clearra_geometry_catalog_realizations_for_clear_state}`; `W/catalog.rs::{instantiate_realization_raw,compile_instantiation_table}`, `GeometryCatalog::{realizations,instantiations,realization_requirement_is_satisfied}`. ILC·BuildUp·support·replay가 소비한다.
- **입출력·불변식:** `(canonical skeleton,realization alternative,D)`에서 `(lock mask,rotation,x,lock_y)`를 얻는다. 필요한 삭제 행 포함, 자기 target 행 삭제 금지, 삭제된 아래 행 수에 따른 y 이동, **재투영 target cells의 정확 일치**를 모두 만족해야 한다.
- **강도 차이:** Rust `skeleton_requirement_domains`는 어떤 alternative의 `required_deleted_rows`가 D에 포함된다는 약한 조건이다. C `skeleton_supports_clear_state`의 exact-state bits/정확 instantiation과 같지 않다. 필요조건을 정확 domain API의 성공으로 바꾸지 않는다.
- **공유·무효화:** `(catalog owner,skeleton ID,D)`에서 모든 구체 alternative를 보존한다. Rust의 사전 instantiation 표는 상한·할당 조건 밖에서 raw iterator로 fallback한다. catalog/registry/projection 계약이 달라지면 ID와 결과를 재사용하지 않는다. C의 template/operation ID·projection evidence까지 소비하는 경로는 Rust tuple만으로 parity를 선언하지 않는다. [S12]

#### OP05 — 접지·cell incidence·행 contributor: G02·G03·A05

- **기존 구현·소비자:** `GeometryCatalog::support`, `W/buildup.rs::placement_is_grounded`; C `buildup/grounded_filter.c::clearra_buildup_grounded_filter_accepts`, `buildup/realization_domain_propagation.c::clearra_realization_deleted_rows_for_state`; Rust `RealizationFeasibilityWorkspace::dependency_relaxation_is_infeasible`의 row contributors.
- **입출력·불변식:** placement가 바닥에 닿거나 한 칸 아래 placement가 board와 교차해야 접지한다. 접지는 필요조건이다. 행 contributor가 모두 실행되면 줄 삭제를 유도하며 같은 kind의 서로 다른 operation도 별개 contributor다.
- **공유·무효화:** 정확 realization/D별 아래 외곽과 가능한 supporter OR, 원래 행 contributor를 공유한다. D 변경으로 위치가 바뀌거나 prefix의 concrete supporter가 달라지면 갱신한다. 같은 residual/counts만으로 support 관계를 공유하지 않는다.
- **남은 작업:** 양쪽 코어에 이미 있는 접지 reject를 추가하는 것이 아니라, 타일링 이전에 supporter 대안과 직접/clear 간접 관계를 준비하여 비관련 쌍 생성을 줄인다. 완전한 저비용 all-mino relation은 아직 신규 개발 범위다. [S14]

#### OP06 — APDP·SameTile·Hall domain: A02~A05·A08

- **기존 구현·소비자:** `W/geometry_domain.rs::{DomainPropagation::compile,compile_minimum,exact_parent_rows,apdp_domain_is_complete,hall_impossible}`; `W/geometry_apdp.rs::ExactArmPairIndex`, `GeometryCatalog::apdp_row_is_static_exact`; C `apdp/geometry_apdp.c::{clearra_geometry_apdp_compile_support_flags,clearra_geometry_apdp_propagate}`, `pruning/geometry_parent_hall_bound.c::clearra_geometry_parent_hall_bound_propagate`.
- **입출력·불변식:** `(catalog,remaining cells,feasible piece mask)`에서 concrete parent domain을 얻고, Hall 판정에는 잔여 공급 수량의 상한도 반영한다. SameTile은 모든 가용 full placement의 공통 셀에서 유도한다. APDP prune은 모든 full parent가 static-exact 지원 범위에 포함됨을 확인한 경우만 허용한다.
- **공유·무효화:** child residual에서 제거된 parent가 실제로 영향을 주는 cell/partial만 갱신한다. 공급 가능 kind 변화는 domain을, 잔여 수량 변화는 Hall 상한을 갱신하며 branch rollback은 이전 parent 대안을 정확히 복구한다. partial 모양을 공유해도 concrete full parent들을 kind 하나로 합치지 않는다.
- **남은 작업:** 기존 full-domain 반복 스캔을 증분 전파로 줄인다. 고급 경로가 비활성인 빈 P7P4에 효과를 주장하지 않는다. static 활성 범위의 손익과 temporal parent 확장은 별도 과제다. [S4][S14]

#### OP07 — residual family와 completion: I07·G13·Q01

- **기존 구현·소비자:** `W/geometry.rs::{ResidualKey,AvailableRowKey,GeometryCompletionOracle}`, `W/geometry_family.rs::GeometrySolutionFamily`의 `Append/Union/Product`; C `packing/geometry_residual_memo.h`, `geometry_solution_family.c`.
- **입출력·불변식:** 고정된 catalog·target/prefix 계약 안에서 `(remaining,packed_counts)`가 suffix family를 찾는다. available-row key는 `after_line_clear`도 포함하며 permutation-closed geometry와 post-clear residual compiler 상태를 분리한다.
- **공유·무효화:** node ID와 memo 값은 해당 compiler/family owner의 namespace에 속한다. 삽입/인터닝을 하지 못하면 정상 계산으로 되돌아간다. geometry residual이 같아도 prefix의 support·통로·row contributors·goal 이력은 다를 수 있다.
- **남은 작업:** 새 residual memo를 추가하지 않고 기존 suffix에 **경계 조건을 인자로 받는 relation fragment**를 연결한다. family ID만으로 완성된 movement 결과를 캐시하지 않는다. 독립성을 증명하지 않은 `Product`에 확률 곱을 붙이지 않는다. [S2][S4][S15]

#### OP08 — 물리 도달성 템플릿과 질의 권한: O05·G04~G09·M06

- **기존 구현·소비자:** `W/reachability.rs::{SharedReachabilityTemplates,ReachabilityTemplate,ReachabilityWorkspace,ReachabilityCache}`; C `reachability/*`, `buildup/buildup_workspace.c::clearra_buildup_workspace_prepare`.
- **입출력·불변식:** 불변 rule/shape/layout에서 전이 템플릿을 만들고 board/target으로 Boolean·lock family·witness·spin/finesse evidence를 구한다. partial search의 positive lock은 재사용할 수 있지만 누락된 lock은 exhaustive 부정 증거 없이 unreachable로 만들 수 없다. Boolean proof는 전체 lock family·증거·multiplicity를 대체하지 않는다.
- **공유·무효화:** Rust shared template는 request 소유이며 board/cache/frontier는 worker private다. dimensions/profile 변경은 template와 cache를 함께 무효화한다. C workspace도 layout/rule descriptor 변경 시 reachability·lock·transition cache epoch를 갱신한다. 호출자가 고정한 owner 조건을 key 밖에 둔 것과 무시한 것은 다르다.
- **남은 작업:** 템플릿 공유는 이미 존재한다. 신규 목표는 보편적 무영향·진입 증명으로 board별 질의를 생략하고 unresolved 부분만 기존 정확 커널로 보내는 것이다. proof를 만들기 위한 topology 순회도 구성 비용에 포함한다. [S13][S15]

#### OP09 — 물리 전이와 terminal evidence: G10~G12·M03·M04

- **기존 구현·소비자:** C `buildup/buildup_geometry_transition_cache.h/.c`; Rust `W/buildup.rs::{BuildOrderGraph,BuildEdge,CandidateProjection,exact_scoring_execution_graph}`, `W/result.rs::prepare_exact_scoring_execution_batch`.
- **입출력·불변식:** `(board,concrete operation,D,rule owner,trace/transition mode)`에서 다음 board/D와 lock/clear 증거를 얻는다. C key는 pose/mask/operation ID, reachability-relevant state, required D, cleared lines 및 trace/transition mode를 구별한다. 공급이 달라도 물리 전이는 같을 수 있지만 goal에 필요한 label은 유지한다.
- **공유·무효화:** C root cache는 initial board·candidate/canonical operation set·trace mode도 비교한다. Rust 결과 경로는 buildable identities를 모은 뒤 identity별 scoring graph를 다시 만들고 보유한다. 후보 검증에서 이미 확보한 증거를 해당 소비자까지 전달할 기회다.
- **남은 작업:** Boolean/첫 witness와 모든 execution label을 구분해 필요한 증거만 생산·이전한다. B2B-save·chain·score 정책은 기존 권위를 유지하고 물리 event와 분리한다. 단순 첫 witness 저장으로 모든 실행의 scoring coverage를 대체하지 않는다. [S6][S15]

#### OP10 — 공급·hold·언어 coverage: Q01~Q05

- **기존 구현·소비자:** `crates/clearra-supply/src/execution_automaton/supply_execution_automaton.rs::{SupplyExecutionAutomaton,SupplyExecutionState,SupplyExecutionMemoKey}`; 기존 `hold_automaton`은 canonical automaton의 type alias다. `W/standard_bag_coverage.rs`, `W/standard_bag_product_memo.rs`; C `supply/{hold_automaton,standard_bag_automaton,supply_state}.c`가 대응한다.
- **입출력·불변식:** 공급 상태와 Current/SwapHeld/StoreCurrent 선택을 소비 미노·다음 공급 상태로 바꾼다. 일반 상태에는 source identity/kind, cursor, hold policy/empty/piece, bag epoch/remainder, observation, provenance가 있다. 확률은 성공 실행의 합이 아닌 성공 큐의 합집합이다.
- **공유·무효화:** `StandardBagProductMemoKey`의 `(language_node,depth,bag_remainder,hold_code)`는 다른 조건이 고정된 worker request/epoch 내부에서만 충분하다. `StandardBagCoverage::supports`는 `Standard7BagLexicographic`, fresh source 등의 범위를 확인한다. 이를 arbitrary supply로 일반화하거나 ID를 다른 owner에 이동하지 않는다.
- **남은 작업:** relation의 accepted language를 기존 공급 product에 연결한다. graph 전용 hold automaton을 새로 만들지 않는다. 같은 kind의 occurrence·hold 분기와 큐 overlap을 보존하며 필요한 경우에만 개별 큐를 materialize한다. [S16]

#### OP11 — 취소·소유권·완료 상태: O06·R06·M03~M06

- **기존 구현·소비자:** `clearra-core-domain/src/execution_cancellation.rs::ExecutionControl`, `clearra-core-ffi/src/memory/{native_core_context,native_scope}.rs`; C `memory/*`; Rust catalog/template/workspace/result 소유자들. native memory facade는 이미 C 포인터를 감싼 RAII다.
- **계약:** cache admission 실패는 miss, 선택적 증명 실패는 Unknown, 필수 결과의 자원 부족은 incomplete/error, 모든 대안의 완전 부정만 infeasible이다. `RealizationFeasibilityWorkspace`의 generation은 후보별 증거 수명을 제한하며 stale graph는 fail-open한다.
- **현재 범위:** `W/realization_feasibility.rs`의 `PRODUCT_COMPLETE_REALIZATION_FEASIBILITY_PREPASS_ENABLED`는 감사 SHA에서 `false`다. 제품경로의 해당 complete prepass는 0개 상태를 탐색한 Unknown을 반환한다. 자료형 존재를 이미 활성화된 완전 의존성 그래프의 증거로 쓰지 않는다.
- **남은 작업:** 중복 소유와 후속 graph 재생성을 줄이고 요청·worker·후보·결과의 수명을 맞춘다. C arena/pool/scratch 이름을 Rust로 복제하지 않는다. Rust 전환 자체에 속도·peak 개선을 보장하는 의미를 부여하지 않는다. [S3][S14][S17]

#### OP12 — 제품 서비스·ABI cutover: R01~R12·H01~H05

- **기존 경로:** `crates/clearra-app/src/app_services.rs::{execute_search_backend_with_control,execute_build_coverage_with_control}`, `clearra-core-ffi/src/problem/generic_buildup.rs`, CLI routing 및 호스트 adapter.
- **확인한 격차:** NativeCore는 execution constraints와 supplied colored identity filter를 거절한다. WasmCpu의 legacy build-coverage 서비스는 `wasm_cpu_build_coverage_not_connected`다. 반대로 C generic BuildUp도 Board128/Wide를 unsupported로 판정한다. 따라서 모든 C 범위를 Rust가 새로 구현해야 한다는 추정도 옳지 않다.
- **분류·전환:** `양쪽 지원 / C만 지원 / Rust만 지원 / 커널은 있으나 서비스 미연결 / 양쪽 미지원`을 요청별로 기록한다. FFI에서 ID 인코딩만 가져오는 의존성과 실제 native 함수 호출도 분리한다. 마지막 consumer가 사라지고 해당 결과·오류·해제 계약이 검증된 기능부터 퇴역시킨다.
- **남은 작업:** 기능별 identity, coverage, CountAll, terminal evidence, cancellation/resource completeness를 맞추고 기존 Rust 계산을 서비스에 연결한다. backend enum 변경·crate rename·C 파일 수 감소만으로 전환 완료를 선언하지 않는다. [S1][S18]

### 7.4 재사용 동치와 증거의 범위

공유 key가 짧은 이유가 owner에 고정된 계약 때문이면, 결과를 다른 수명으로 옮길 때 그 계약도 함께 옮겨야 한다. hash/digest만으로 서로 다른 실제 key를 동일시하지 않는다. 이미 조건이 고정된 호출마다 큰 key를 복제하라는 지침은 아니다.

| 관측상 같은 값 | 공유 가능한 대표 계산 | 추가로 같아야 하는 조건 |
| --- | --- | --- |
| 같은 piece kind | 공급 종류 집계 | concrete placement·occurrence·hold 분기까지 동치는 아님 |
| 같은 target cell 소유 | skeleton과 geometry family | D·realization·물리 pose·evidence는 별도 |
| 같은 physical lock mask | 해당 범위의 collision/grounding | orientation·ordered kick·last rotation·spin/finesse |
| 같은 board와 D | owner가 고정된 물리 전이 | remaining operation·supply·goal history·결과 권한 |
| 같은 residual과 counts | 같은 compiler의 geometry suffix | prefix supporter·통로·clear contributors·goal 경계 |
| 같은 language node ID | 같은 worker/epoch의 product memo | family namespace·universe·weight·관측/공급 계약 |

이 구분이 필요한 이유는 “연산 결과의 재사용”과 “문제를 더 약하게 관측한 값의 우연한 일치”가 다르기 때문이다. 기존 함수 이름을 바꾸는 작업보다 이 동치·무효화 계약을 맞추는 일이 C→Rust 범위의 큰 부분이다.

## 8. Google Rust 가이드의 적용 범위

공식 가이드의 일반 원칙과 플랫폼 전용 규칙을 구분한다. 핵심 채택 항목은 rustfmt, 명확한 타입·오류·소유권, 불필요한 깊은 복사 회피, 필요한 경우의 trait, 최소 unsafe 경계다. Chromium 전용 작은 crate·평면 파일 배치·async/thread 규칙을 Cargo 프로젝트에 일괄 적용하지 않는다. stable 도구로 지원하지 않는 포맷 옵션은 가이드가 허용하는 범위에서 제외한다. [E1]

Clearra에 적용할 설계 결정은 다음과 같다.

| Clearra 결정 | 적용 이유와 완료 기준 |
| --- | --- |
| 기존 crate를 먼저 사용 | 공통 의미 소유자와 의존성 방향이 명확해야 새 crate를 제안할 수 있음 |
| `struct`/`enum`과 `impl`로 불변식 캡슐화 | invalid ID/frame/proof 상태를 외부에서 임의 조립하지 않되 클래스식 계층을 강제하지 않음 |
| trait는 실제 교체·generic 경계에 한정 | 파일 정리만을 위한 trait, 모든 hot call의 동적 dispatch를 만들지 않음 |
| 불변 요청 데이터와 worker 가변 상태 분리 | 실제 수명이 드러나고, 불필요한 clone과 중복 컴파일이 사라짐 |
| 필요한 공유 소유권은 명시적으로 허용 | 여러 워커가 같은 불변 owner를 사용할 때 무리한 lifetime 또는 unsafe로 대체하지 않음 |
| 기존 safe-core 정책 유지 | ABI 등 불가피한 경계만 별도 검토; workspace 전체 금지를 느슨하게 만들지 않음 |
| 공개 API와 내부 구현의 안정성 구분 | 외부 호환 계약을 보존하면서 내부 구현을 정리할 수 있어야 함 |
| 폴더·파일은 실제 연산·소유권에 맞춤 | 관련 타입과 hot 연산을 함께 둘 수 있고, SRP 경계를 넘는 융합도 시간·메모리와 정확성의 근거로 판단 |

위 표는 Clearra의 적용 결정이며, 모든 항목이 Google의 보편적 강제 규칙이라는 뜻은 아니다. 기존 `Cargo.toml`은 edition 2021, workspace `unsafe_code = forbid`를 선언하며 FFI에는 예외가 있다. edition 변경·전 저장소 재포맷을 성능 작업에 섞지 않는다. [S7]

## 9. 확률·조건·호스트의 통합

물리 관계가 완성되면 기존 piece-order language와 공급 계산을 재사용한다. 성공 큐를 한 번씩 세는 확률과 모든 build/hold variant를 세는 CountAll은 다른 집계다. 동일 kind 때문에 나눗셈으로 순열 중복을 대충 보정하거나, linear extension 수를 성공 큐 수로 대체하지 않는다.

독립 component는 실제 language·hold·bag·목표 상태의 합성 조건을 갖는다. 출력으로 모든 개별 큐나 trace를 요구하면 출력량 자체의 비용은 남는다. 확률만 필요한 모드는 표현 가능한 공통 잔여 언어와 가중치를 합성하여 큐·배치 prefix materialization을 줄이는 별도 경로로 둔다.

Pages GUI, Discord, CLI의 속도 차이는 같은 canonical 요청, 같은 core/프로필/가속, 같은 논리 워커 정책, 같은 출력·증거 계약을 고정한 뒤 비교한다. Web의 producer/verifier 및 직렬화, Discord의 native process와 `includeSolutionData`, CLI build feature 등은 실제 경로별로 기록한다. 현재 배포 설정이나 속도 비율을 이번 소스 조사만으로 확정하지 않는다. [S8]

코어 시간과 호스트의 초기화·수송·merge·render 시간을 따로 측정한다. 호스트별로 solver를 복제하여 차이를 메우지 않는다. 공통 core와 요청 계약이 정리된 뒤 adapter별 불필요한 재계산·중복 수송을 줄인다.

## 10. 파일·모듈 정리 기준

파일 감소를 다음 네 가지로 분리한다.

| 대상 | 조치 | 보존할 경계 |
| --- | --- | --- |
| build·다운로드·의존성 산출물 | 생성 위치·재현성·보존기간에 따른 운영 정리 | qualification 증거와 재현 불가능 자료 |
| 퇴역 C·중복 solver·중복 adapter | 기능 parity와 소비자 확인 후 삭제 | 독립 oracle, 필요한 외부 ABI |
| 과도하게 작은 wrapper·helper | 같은 변경 이유·수명·불변식을 가진 기존 모듈로 통합 | 외부 공개 경로·독립 책임 |
| 너무 큰 복합 모듈 | 의미별 owner와 공개 면을 먼저 정한 뒤 분리 | 하나의 알고리즘에 필요한 응집도 |

테스트 fixture·golden·회귀 사례는 파일 수를 줄이기 위해 합쳐 의미를 잃지 않는다. 같은 행동군의 작은 테스트 파일은 owner별로 통합할 수 있으나 서로 다른 정확성 근거를 삭제하지 않는다. vendor는 upstream 구조·라이선스·업데이트 경로를 존중한다.

정리 대상의 판단 기준은 다음과 같다.

- 같은 변경 때문에 항상 같이 바뀌는가, 아니면 서로 독립적으로 바뀌는가.
- 동일 규칙·좌표 변환·검증을 여러 파일이 따로 정의하는가.
- 공개 API와 소비자가 실제로 독립적인가.
- 작은 helper가 상태의 소유권을 숨기거나 파일 간 이동만 늘리는가.
- 큰 모듈이 기하·물리·공급·출력·호스트 수명을 동시에 소유하는가.
- 통합 후에도 검색·리뷰·테스트 소유권이 더 명확해지는가.

**SRP는 `AGENTS.md`의 작업 규약으로 남기며 자동 합격 기준으로 만들지 않는다.** 과거 문서의 helper shell, action-per-file, 1,000줄 제한, `SRP rationale:` 표식 요구를 복구하지 않는다. 단순 문구·marker·파일 배치 검사를 늘려 이 계획의 완료를 주장하지 않는다. 현재 `validate_srp_policy.ps1`는 PowerShell 문법 검사이고 `validate_file_size.ps1`는 검사 대상 열거다. 역사적인 파일 이름을 SRP 검증의 근거로 해석하지 않는다. 실제 의존성·ABI·unsafe 경계의 계약 검사와 실행한 정확성 검증은 별도로 유지한다. [S9]

Rust의 모듈 분리는 실행 시간과 메모리 효율성을 위한 수단이다. 연산을 분리하면 동일한 projection·domain·증거를 다시 만들거나 중간 상태를 더 오래 보유해야 하는 경우에는 함께 두거나 융합한다. 이는 강제 SRP 예외가 아니라 정상적인 설계 선택이다. 줄 수를 낮추기 위한 파일 분할, 한 작업당 새 파일·crate 생성, 형식적 경계를 위한 중간 객체 도입을 하지 않는다.

파일 이동·rename은 관련 기능의 계약이 안정된 뒤 별도 변경으로 진행한다. 같은 `geometry.rs`나 `app_services.rs`를 기능 담당자와 구조 담당자가 동시에 고치지 않는다. 일괄 대이동으로 후속 정확성 비교와 병렬 작업을 막지 않는다.

## 11. 작업 분해: 88개의 기본 작업 단위

각 항목은 후속 구현 작업이다. `선행`은 반드시 끝나야 하는 계약·작업 ID이며, 같은 파일의 편집권은 12절의 규칙을 추가로 적용한다. 작업이 크면 **계약 확정 → 한 소비자 연결 → 나머지 연결·중복 제거**로 더 나누되, 작업마다 파일을 새로 만들지 않는다. 경로는 기준 SHA의 수정 후보이며 소유권 확정 후 변경할 수 있다.

V01·V02·V03 같은 검증 작업은 전체 항목이 끝날 때까지 기다리는 전역 장벽이 아니다. `V01[ILC]`, `V01[APDP]`, `V01[support/clear]`, `V02[기본 규칙]`, `V02[새 kick 규칙]`, `V03[해당 제품 기능]`처럼 해당 입력이 준비되는 즉시 분리 실행한다. 관계 연결과 C 전환도 같은 범위의 통과 증거를 소비한다. 전체 지원 범위의 합집합은 최종 V06에서 점검한다.

연구 표시는 증명과 손익 검토를 끝내야 채택할 수 있는 후보다. 연구가 성립하지 않으면 현재 정확한 범위를 유지하고 근거를 기록한다. 연구 후보 전체를 첫 성능 개선의 필수 선행조건으로 삼지 않는다.

### F. 공통 계약·기준선 — 8개

- [ ] 수정 파일: `docs/architecture.md`, `docs/dependency-boundary.md`, 구현 내용: **F01 — 실행 경로와 목표 Rust 소유권을 동결한다.** 선행: 없음. 완료: native/WASM/CLI/Discord의 실제 feature·backend와 현재/목표 구조가 구분되고 기존 C 전제의 대체 범위가 명시된다.
- [ ] 수정 파일: `docs/research/rust-core-and-relation-roadmap-2026-11-20.md`, `crates/clearra-core-domain/src`, 구현 내용: **F02 — 배치·행·공급·결과의 동등성 계약을 고정한다.** 선행: 없음. 완료: concrete ID, selected/executed, CountAll, hold, coverage, terminal label, Unknown/Incomplete와 7.4절의 동치 단계가 확정된다. 필요조건·exact 결과 및 Boolean·증거 family의 권한을 구분한다.
- [ ] 수정 파일: `docs/dependency-boundary.md`, 구현 내용: **F03 — 공통 의미별 단일 소유자와 허용 의존성 표를 확정한다.** 선행: F01, F02. 완료: 7.3절 연산군별 기존 소유자, 입력·출력, key 안팎의 고정 계약, lifetime과 무효화가 정해지고 scoring 등 제품 경계가 보존된다. 연산군마다 새 객체·파일·crate를 만들지 않는다.
- [ ] 수정 파일: `docs/architecture.md`, `Cargo.toml`, 구현 내용: **F04 — Google Rust 가이드 적용 결정을 문서화한다.** 선행: F03. 완료: 일반/플랫폼 전용 규칙, stable 도구, unsafe 예외, 공개 API와 내부 코드의 기준이 구분된다. 이 단계에서 전역 재포맷·edition 변경을 하지 않는다.
- [ ] 수정 파일: `docs/research/rust-core-and-relation-roadmap-2026-11-20.md`, 구현 내용: **F05 — 파일 현황의 분류·기준 SHA·소유자 목록을 확정한다.** 선행: 없음. 완료: tracked source/test/vendor/generated를 재현 가능하게 집계하고 삭제 후보와 실제 퇴역 조건을 구분한다.
- [ ] 수정 파일: `crates/clearra-core-executor/examples/placement_graph_bench.rs`, 구현 내용: **F06 — 비교 요청과 출력 계약을 고정한다.** 선행: F02. 완료: P7P4의 10배치/11공급, hold, profile, 출력·증거 모드, 가속, 자동 워커 정책, 성공 결과 검산값이 하나의 비교 계약에 들어간다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/result.rs`, `tools/clearra-manage/src/runtime.rs`, 구현 내용: **F07 — 연산·수명·실행 진단 항목의 의미를 정의한다.** 선행: F02. 완료: 미실행·미보고·0회가 구분되고 hot path 노드별 타이머 없이 관계 구성·fallback·보유량을 비교할 수 있다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_reference_tests.rs`, `crates/clearra-invariant-tests/src/lib.rs`, 구현 내용: **F08 — 독립 oracle과 제품 반례의 보존 범위를 확정한다.** 선행: F02. 완료: 새 공통 helper를 양쪽에서 공유한 자기검증만 남지 않고 C 퇴역 뒤에도 독립 검증 근거가 유지된다.

### O. 공용 객체·의미 원천 — 8개

- [ ] 수정 파일: `crates/clearra-geometry/src`, `crates/clearra-core-executor/src/backend/wasm_cpu/reachability.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/extended_reachability.rs`, `crates/clearra-finesse/src/lib.rs`, 구현 내용: **O01 — 중복 kick·회전 중심 정규화를 공통 소유자로 옮긴다.** 선행: F03. 완료: 세 소비자가 같은 정의를 사용하며 ordered kick·좌표 의미가 동일하다. 실제 소유자는 crate 의존성을 확인해 확정한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/kick_profiles.rs`, `crates/clearra-core-ffi/src/rules`, 구현 내용: **O02 — 프로필 ID의 의미와 ABI 인코딩을 분리한다.** 선행: F03. 완료: Rust 코어가 FFI crate를 프로필 의미의 원천으로 참조하지 않으며 기존 replay ID 호환성이 유지된다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/row_frame.rs`, `crates/clearra-core-executor/src/conditioned_local_relation.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs`, 구현 내용: **O03 — 기존 행 좌표 연산을 ILC·관계의 공통 계약으로 확정한다.** 선행: F02, F03. 완료: 순수 행사상과 폭 10·높이 1~6/terminal wrapper를 구분하고 C의 최대 16행 계약을 좁히지 않는다. 같은 의미의 변환을 연결하되 일반화가 필요한 부분은 범위별로 검증한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, 구현 내용: **O04 — cell incidence의 소유자를 카탈로그로 통일한다.** 선행: I01. 완료: 같은 cell→placement 목록의 중복 생성이 제거되고 각 소비자의 수명·실제 보유 비용이 명확하다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/mod.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/reachability.rs`, 구현 내용: **O05 — 불변 compiled 데이터의 요청 소유권을 통일한다.** 선행: F03, O02. 완료: 기존 `SharedReachabilityTemplates`와 catalog owner를 재사용하며 남은 중복 생성만 제거한다. dimensions/profile 변경에 따른 무효화와 worker-private 결과의 범위를 보존한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/parallel_worker.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs`, 구현 내용: **O06 — 가변 workspace를 워커 수명에 맞춘다.** 선행: F03, O05. 완료: worker private 상태와 candidate 임시 상태가 분리되고 취소·완료 후 참조가 요청 전체를 붙잡지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/extended_board.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/extended_reachability.rs`, 구현 내용: **O07 — compact/extended의 공통 의미와 특수화 경계를 정한다.** 선행: O01, O03. 완료: 공유 가능한 규칙·projection은 재사용하고 크기별 능력·실행 커널은 필요한 경우 유지한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/mod.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/mod.rs`, 구현 내용: **O08 — 기존 Rust CPU 구현을 공통 서비스 면으로 노출한다.** 선행: O01, O02, O03, F03. 완료: 새 solver 없이 기존 geometry/verification을 native와 WASM가 소비할 수 있고 공개 호환 경로가 명시된다.

### R. C→Rust 제품 전환 — 12개

- [ ] 수정 파일: `core-c/include`, `crates/clearra-core-ffi/src/raw/bindings.rs`, 구현 내용: **R01 — C 심볼의 실제 소비자를 분류한다.** 선행: F01. 완료: 내부 전용·확인된 외부 ABI·qualification 전용을 나누고 유지해야 할 layout·ownership 계약이 정해진다.
- [ ] 수정 파일: `crates/clearra-app/src/app_services.rs`, `crates/clearra-cli/src/cli_routing.rs`, 구현 내용: **R02 — 제품 요청별 C/Rust 기능 격차를 fixture에 연결한다.** 선행: F01, F02. 완료: 양쪽 지원·C만 지원·Rust만 지원·커널 존재/서비스 미연결·양쪽 미지원을 구별하고 기존 연산을 다시 구현하지 않는다. build-coverage 미연결과 양쪽의 capability 거절을 같은 기능 결손으로 묶지 않는다.
- [ ] 수정 파일: `crates/clearra-app/src/app_services.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/build_probability.rs`, 구현 내용: **R03 — 기존 build coverage 진입점의 Rust 연결 격차를 닫는다.** 선행: R02, O08. 완료: 기존 Rust supply/coverage 계산을 재사용하여 요청·결과·완전성 계약을 만족한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/extended_geometry.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/extended_buildup.rs`, 구현 내용: **R04 — 확인된 확장 보드·규칙·generic 기능 격차를 하나씩 연결한다.** 선행: R02, O07, O08. 완료: 현재 지원 계약을 잃지 않으며 미지원 조합을 조용히 축소하거나 fallback하지 않는다. 기능 가족별로 하위 작업을 배정한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/setup_finder.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/extended_pc_search.rs`, 구현 내용: **R05 — setup/checkpoint/continuation 제품 경로를 공통 Rust 실행에 연결한다.** 선행: R02, O08. 완료: 목표 checkpoint·관측 정책·해 identity가 기존 계약과 같다. 서로 다른 요청 가족은 별도 변경으로 제출한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/result.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/parallel_worker.rs`, 구현 내용: **R06 — 취소·budget·부분 결과의 실행 계약을 통일한다.** 선행: R02, O06. 완료: 자원 부족과 불가능이 구별되고 모든 host가 같은 completeness를 소비한다.
- [ ] 수정 파일: `crates/clearra-core-ffi/src`, `core-c/include`, 구현 내용: **R07 — 확인된 외부 소비자에 한해 Rust ABI adapter를 연결한다.** 선행: R01, R03, R04, R05, R06. 완료: 필요한 심볼·레이아웃·해제·오류 계약이 유지되고 import/export 중복 링크가 없다. 외부 소비자가 없으면 내부 호출 제거로 완료할 수 있다.
- [ ] 수정 파일: `crates/clearra-app/src/app_services.rs`, `crates/clearra-cli/src/cli_routing.rs`, 구현 내용: **R08 — 검증된 기능부터 기본 CPU 서비스를 Rust로 전환한다.** 선행: V03의 해당 제품 기능 gate. 완료: 선택된 backend·capability가 실제 실행과 일치하고 미지원 기능은 전환하지 않는다. 다른 기능 가족의 전환을 불필요하게 기다리지 않는다.
- [ ] 수정 파일: `crates/clearra-cli/Cargo.toml`, `crates/clearra-app/Cargo.toml`, `.github/workflows/release-cli.yml`, 구현 내용: **R09 — 제품 빌드·배포의 feature와 링크를 Rust 경로에 맞춘다.** 선행: R07, R08. 완료: native/WASM 패키지의 실제 링크·서비스 선택이 계획과 일치한다. 배포 실행은 별도 gate를 따른다.
- [ ] 수정 파일: `crates/clearra-core-ffi/Cargo.toml`, `core-c/CMakeLists.txt`, 구현 내용: **R10 — C 기준 구현을 qualification 전용으로 격리한다.** 선행: R09, V03. 완료: production이 C solver를 필요로 하지 않으며 독립 검증의 진입점은 유지된다.
- [ ] 수정 파일: `core-c`, `crates/clearra-core-ffi/src/native`, 구현 내용: **R11 — 증거가 확보된 프로젝트 C 구현을 퇴역시킨다.** 선행: R10, 해당 기능의 V03·H05·성능 gate; 전체 solver 퇴역에는 V05. 완료: production 소비자가 없고 해당 계약의 증거가 충족된 부분만 제거한다. 독립 ABI shim을 다른 관계 연구 때문에 붙잡지 않으며, 독립 oracle 대체 근거 없이 일괄 삭제하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-ffi/src/memory`, `scripts/architecture`, `docs/architecture.md`, `docs/dependency-boundary.md`, 구현 내용: **R12 — 퇴역 wrapper·C 빌드 규칙·문서의 잔여 전제를 정리한다.** 선행: R11, S07. 완료: 사용하지 않는 두 번째 메모리 체계와 실행 경로가 남지 않고 다른 정확성·제품 경계 gate는 유지된다.

### I. ILC의 정확한 재사용 — 8개

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, 구현 내용: **I01 — 약한 필요조건과 exact realization-domain의 소비자를 분리한다.** 선행: O03. 완료: Rust requirement-domain과 C exact clear-state-domain의 강도 차이, 자기 행 삭제·재투영 조건이 명시되고 소비자가 올바른 view를 사용한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, 구현 내용: **I02 — exact frame-domain의 중복 유도를 제거한다.** 선행: I01. 완료: 같은 `(placement, D)` 판정은 한 권위에서 나오며 약한 requirement domain으로 대체하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs`, 구현 내용: **I03 — 실제 사용되는 frame의 instantiation을 재사용한다.** 선행: I02. 완료: 전체 D 선계산과 반복 raw 계산의 비용을 요청 특성별로 비교하여 불필요한 계산을 생략하고 정확한 결과를 공유한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/inverse_projection.rs`, 구현 내용: **I04 — 기호적 ILC domain의 필요충분성을 연구한다.** 선행: I01, F08. 완료: 표준 범위에서 삭제 필요·자기 행 금지·정확 투영을 포함하는 증명 또는 반례가 남는다. 미증명 범위는 기존 exact path를 유지한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/inverse_projection.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs`, 구현 내용: **I05 — 반복 shape/row assignment의 투영을 공유한다.** 선행: I02. 완료: 기존 `ProjectionRowFilter`를 재사용하고 의미가 완전한 공유 key에서 새 projection 생성 수가 감소한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, 구현 내용: **I06 — clear 사건 이후 영향받는 frame만 갱신한다.** 선행: I03, G03. 완료: before/after-clear domain과 이미 배치된 surviving cells를 구분하여 정확한 무효화가 이뤄진다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_family.rs`, 구현 내용: **I07 — family의 매개변수화된 관계 경계 계약을 설계한다.** 선행: I02, F02. 완료: suffix 자체와 prefix-dependent owner/clear 조건이 분리되고 family ID만으로 이동 관계를 캐시하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/extended_inverse_catalog.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/extended_geometry_domain.rs`, 구현 내용: **I08 — 증명된 ILC 재사용을 extended 범위로 확장한다.** 선행: I04, I05, O07. 완료: 지원 범위별 증명·oracle을 갖추고 compact 한도를 전체 제품 한도로 잘못 고정하지 않는다. 연구 후 채택 과제다.

### A. APDP·도메인 전파 — 8개

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs`, 구현 내용: **A01 — 실제 APDP 활성 범위를 진단에 드러낸다.** 선행: F07. 완료: 빈 필드·nonempty·resource-authoritative 경로의 full/minimum domain 실행이 구별된다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_apdp.rs`, 구현 내용: **A02 — 기존 exact parent 대안을 관계 계층에 제공한다.** 선행: F02, O04. 완료: concrete parent ID와 대안·static-exact 범위가 보존되고 새 parent index를 중복 제작하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_domain.rs`, 구현 내용: **A03 — 변경된 parent만 전파하는 증분 domain을 도입한다.** 선행: A01, A02. 완료: 각 branch의 살아 있는 parent 집합이 full compile과 같고 영향 없는 cell/partial의 재검사가 줄어든다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_domain.rs`, 구현 내용: **A04 — SameTile·공통 owner 증거의 무효화와 되돌리기를 완성한다.** 선행: A03. 완료: parent 제거·복구 후 stale proof가 없고 pivot·불가능 판정이 full compile과 같다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_domain.rs`, 구현 내용: **A05 — 미래 부모·frame까지 불가능한 support만 부분 타일링에서 prune한다.** 선행: A04, G02, I02. 완료: 현재 받침 부재만으로 후보를 버리지 않고 최종 후보 identity·coverage가 보존된다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_apdp.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/extended_geometry_domain.rs`, 구현 내용: **A06 — temporal APDP의 정확한 적용 범위를 연구한다.** 선행: A02, I04. 완료: parent별 clear-domain 상관관계의 증명 또는 제한 사유가 기록된다. 현재 static-exact 제한을 먼저 없애지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_component.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_separator.rs`, 구현 내용: **A07 — component 경계의 관계 합성 조건을 연구한다.** 선행: I07, G09, G13; goal 합성에는 G12. 완료: geometry·movement·clear·goal 독립성이 각각 증명된 범위만 합성되고 공급 coupling이 남는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs`, 구현 내용: **A08 — APDP 활성 범위 확대의 손익을 별도로 판정한다.** 선행: A03, A04, V01의 APDP gate. 완료: 현재 static-exact 범위에서 빈 필드·resource 계약과 손익을 별도 판정한다. temporal 범위 확대만 A06의 증명을 추가 요구하며 I04·A06을 일반 활성 검토의 선행조건으로 만들지 않는다.

### G. 정확한 관계와 사건 — 15개

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/structural_order.rs`, 구현 내용: **G01 — 관계·증거·미완성 상태의 공통 계약을 만든다.** 선행: F02, F03. 완료: OR/부정/선택/실행/사건과 Exact·Unknown이 구별되고 opaque BFS callback이 없다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, 구현 내용: **G02 — 모든 미노의 support OR를 카탈로그 단계에서 준비한다.** 선행: G01, O04, I02. 완료: 바닥·초기 점유·여러 supporter·미래 parent 대안이 정확히 유지되고 Z의 L 또는 S 사례가 표현된다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/build_clear_rows.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, 구현 내용: **G03 — 원래 행 contributor와 atomic clear 사건을 연결한다.** 선행: G01, O03. 완료: lock 직후 동시 삭제와 새 frame이 정확하며 O의 간접 S 영향은 이동 간선으로 잘못 승격되지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_independence.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, 구현 내용: **G04 — 무관한 placement 쌍을 incidence로 생성 전에 제외한다.** 선행: G01, O04, O01. 완료: 보편적인 무영향이 증명된 쌍만 생략하고 증명되지 않은 쌍은 Unknown으로 유지하며 두 blocker의 공동 효과 반례를 보존한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, 구현 내용: **G05 — ordered kick의 first-success 조건을 기호화한다.** 선행: G01, O01. 완료: 앞 후보 실패·선택 후보 성공을 함께 보존하고 뒤 후보만 검사하는 오류가 없다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/placement_poset.rs`, 구현 내용: **G06 — 하드드롭·항상 열린 진입의 충분조건을 일반 미노 규칙으로 묶는다.** 선행: G01, O01. 완료: 여러 후보에서 같은 증명을 재사용하고 충분조건 실패를 Impossible로 바꾸지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/monotone_reachability.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/placement_poset.rs`, 구현 내용: **G07 — 기존 O 규칙을 공통 relation 계약의 특수화로 유지한다.** 선행: G01, O03. 완료: capability·interval 전제와 cold 증명이 유지되고 O Boolean을 spin evidence로 승격하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_independence.rs`, 구현 내용: **G08 — kick prefix가 고정되는 조건을 일반화한다.** 선행: G04, G05, G06. 완료: 관련 없는 owner 변수와 반복 분기 평가를 증명으로 제거하며 프로필별 반례가 남는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_independence.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, 구현 내용: **G09 — 이동 통로·분리 경계의 재사용 규칙을 증명한다.** 선행: G04, G05. 완료: geometry separator와 이동 separator를 구분하고 상승 kick·먼 진입로·복수 차단자를 처리한다.
- [ ] 수정 파일: `crates/clearra-objectives/src/policy/execution_constraint_policy.rs`, `crates/clearra-replay/src/scoring_execution.rs`, 구현 내용: **G10 — 목표 전이에 필요한 중립적인 event 사실 계약을 고정한다.** 선행: F03, G03. 완료: 기존 lock/clear 증거를 재사용하며 요구되는 terminal label과 Unknown이 표현된다.
- [ ] 수정 파일: `crates/clearra-postprocess/src/score_batch/b2b_execution_filter.rs`, `crates/clearra-scoring/src/b2b_preservation.rs`, `crates/clearra-scoring/src/event/spin_detector.rs`, `crates/clearra-spin/src`, 구현 내용: **G11 — 기존 B2B·spin 정책을 중립 조건으로 준비한다.** 선행: G10. 완료: 정책을 solver에 복사하지 않고 기존 판정과 동등한 조건을 생산한다. save의 PC 예외와 수치 chain은 구분한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/result.rs`, `crates/clearra-app/src/app_services.rs`, 구현 내용: **G12 — 관계와 목표 전이를 결합하여 불필요한 증거 계산을 생략한다.** 선행: G03, G05, G10, G11. 완료: 0줄/4줄/PC 등 확정 조건을 먼저 처리하고 필요한 terminal evidence만 계산하며 runtime scoring 직접 의존을 추가하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_family.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs`, 구현 내용: **G13 — Append/Union에 support/clear 조각부터 증분 결합한다.** 선행: I07, A02, G02, G03, G04. 완료: 대안의 상관관계·prefix 경계를 유지하면서 관계 재생성이 감소한다. 이동 조각은 G05·G08·G09의 해당 증명 이후 별도 연결하며, 큰 summary 연구를 초기 완료 조건에 묶지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_dependency_differential_tests.rs`, `docs/research`, 구현 내용: **G14 — 의미 반례와 비용 반례를 별도 축적·최소화한다.** 선행: F08, G01. 완료: 관련 미노·clear 사건·kick 후보로 줄인 재현 사례와 일반화 규칙의 적용 범위를 검토할 수 있다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/structural_order.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs`, 구현 내용: **G15 — 증명된 범위마다 관계 완성과 fallback의 경계를 연결한다.** 선행: G02, G03, G04, G05, G06, G07, G14, V01의 support/clear gate, V02의 기본 규칙 gate. 완료: 해당 범위는 물리 prefix 없이 평가하고 미완성 범위·구성용 physical 호출·fallback을 공개한다. G08·G09·G12·G13은 해당 증명과 검증 후 각각 추가 연결하며 초기 개선을 모든 연구 완료에 묶지 않는다.

### Q. 큐·확률·순열 — 5개

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/piece_order_language.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/structural_order.rs`, 구현 내용: **Q01 — 완성 관계를 기존 순서 언어의 입력으로 연결한다.** 선행: G01, F02. 완료: poset fast subset과 일반 조건부 관계의 구분이 있으며 물리 판정을 재실행하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_product_memo.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/terminal_hold_projection.rs`, 구현 내용: **Q02 — 같은 kind·hold·CountAll의 multiplicity 계약을 결합한다.** 선행: Q01. 완료: 기존 canonical supply automaton을 재사용하고 concrete occurrence와 공급 선택을 보존한다. standard-bag product key의 worker/epoch와 일반 공급의 관측·provenance를 혼합하지 않으며 CountUnique/CountAll의 집계를 구분한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/coverage_product.rs`, 구현 내용: **Q03 — 여러 타일링의 성공 큐를 정확히 합집합한다.** 선행: Q02. 완료: 서로 겹치는 실행 언어를 더해서 확률을 부풀리지 않고 universe·weight가 다른 coverage를 섞지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/build_probability.rs`, 구현 내용: **Q04 — 독립 관계 성분의 가중 합성을 연구한다.** 선행: Q02, G13, A07. 완료: shuffle·hold·bag·goal coupling을 반영하며 단순 확률 곱이나 factorial 보정의 반례를 배제한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/build_probability.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/result.rs`, 구현 내용: **Q05 — 확률 전용 요청에서 큐·물리 prefix materialization을 생략한다.** 선행: Q03, G15, O08. 완료: 실제 제품의 해별/전체 확률이 oracle와 같고 출력 모드가 요구하지 않는 중간 객체가 생성되지 않는다.

### M. 종료 진단·메모리 수명 — 6개

- [ ] 수정 파일: `tools/clearra-manage/src/runtime.rs`, 구현 내용: **M01 — 정상 종료·Unix signal·manager 종료를 구분해 기록한다.** 선행: F07. 완료: 125로 원 신호가 사라지지 않고 외부 제한·사건 미관측은 unavailable로 남는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/result.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/parallel_worker.rs`, 구현 내용: **M02 — shared/worker/candidate/result/transport 수명별 보유량을 관측한다.** 선행: F07. 완료: 객체 모델 추정과 표본 RSS를 구분하고 전체 실패 시 어느 단계까지 진행했는지 확인할 수 있다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/result.rs`, 구현 내용: **M03 — 완료된 단계의 중간 소유권을 결과로 이전·해제한다.** 선행: O05, O06, M02. 완료: 요청 모드별 필수 증거는 남기면서 사용이 끝난 candidate/graph가 다음 단계와 불필요하게 공존하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/result.rs`, `crates/clearra-replay/src/scoring_execution.rs`, 구현 내용: **M04 — 목표 관계가 제공한 증거로 점수 그래프 재구성을 줄인다.** 선행: G12, M03. 완료: 같은 identity의 graph를 무조건 다시 만들고 모두 보유하는 경로를 모드별로 줄이며 exact score 의미를 보존한다.
- [ ] 수정 파일: `crates/clearra-app/src/cooperative_execution.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/result.rs`, 구현 내용: **M05 — 결과 생산·소비 사이의 bounded batch 소유권을 정한다.** 선행: M03, F06. 완료: backpressure와 완전성 계약이 있으며 느린 소비자 때문에 이전 단계 전체를 중복 보유하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs`, 구현 내용: **M06 — 공유 캐시의 owner와 세션 종료 수명을 검증한다.** 선행: O05, M02. 완료: 이전 catalog·request를 불필요하게 붙잡지 않고 재사용 실패가 불가능 판정으로 바뀌지 않는다. 캐시라인·컨테이너 교체 과제는 포함하지 않는다.

### H. 공통 제품·호스트 — 5개

- [ ] 수정 파일: `crates/clearra-app/src`, `crates/clearra-cli-command/src/web_command_parser.rs`, 구현 내용: **H01 — 호스트와 독립적인 canonical 요청·출력 계약을 정한다.** 선행: F01, F06. 완료: 같은 요청이 다른 worker·profile·evidence 모드로 조용히 해석되지 않는다.
- [ ] 수정 파일: `crates/clearra-cli/src/cli_routing.rs`, `apps/clearra-discord-bot/src/clearra/direct-executor.mjs`, 구현 내용: **H02 — CLI와 Discord를 검증된 Rust 서비스에 연결한다.** 선행: H01, R08. 완료: 실제 executable feature·backend·worker·solution-data 설정을 명시하고 두 adapter의 결과 계약이 같다.
- [ ] 수정 파일: `crates/clearra-wasm/src/wasm_command_runtime.rs`, `apps/clearra-web/src/workers/DistributedWasmJobRunner.ts`, 구현 내용: **H03 — Web/WASM의 실행 의미를 공통 CPU 서비스와 맞춘다.** 선행: H01, O08. 완료: host 분산·cooperative yield는 유지하면서 solver·profile 의미의 중복이 없다.
- [ ] 수정 파일: `apps/clearra-web/src/workers`, `apps/clearra-discord-bot/src/job-service/runner.mjs`, 구현 내용: **H04 — 중복 직렬화·전송·결과 보유를 요청 모드별로 줄인다.** 선행: H01, M05. 완료: 필요한 결과는 보존하고 호스트 overhead와 코어 연산 감소를 따로 기록한다.
- [ ] 수정 파일: `crates/clearra-invariant-tests/src`, `scripts/product-e2e.ps1`, `scripts/lib`, 구현 내용: **H05 — GUI·Discord·CLI의 동일 요청을 비교한다.** 선행: H02, H03, H04, V04의 해당 기능 gate. 완료: identity·coverage·completeness가 같고 wall time을 core/host 구간으로 나눠 해석한다. 행동별 E2E 구현은 기존 `scripts/lib/product-e2e-*.ps1` 소유자를 사용한다.

### S. 파일·SRP·문서 정리 — 7개

- [ ] 수정 파일: `docs/architecture.md`, `config/clearra-management.v1.json`, 구현 내용: **S01 — 소스 수와 산출물 수의 관리 기준을 분리한다.** 선행: F05. 완료: 보존할 evidence·재현 가능한 산출물·vendor가 구별되고 자동 삭제로 재현 증거를 잃지 않는다. config 수정은 필요할 때만 별도로 검토한다.
- [x] 수정 파일: `AGENTS.md`, `docs/architecture-validation.md`, `docs/srp-debt.md`, 구현 내용: **S02 — SRP를 작업 규약으로 정리하고 정적 합격 기준을 폐기한다.** 선행: 없음. 완료: 1,000줄 제한·필수 rationale 문구·marker·action-per-file 강제를 복구할 여지가 없고 Rust의 속도·메모리를 위한 연산 융합이 허용된다. 기존 스크립트의 문법 검사/대상 열거와 실제 ABI·의존성·unsafe 계약 및 실행 증거를 구분한다. 새 정적 SRP 검사는 만들지 않는다. 2026-11-20 후속 변경에서 세 문서를 수정하고 실제 스크립트의 범위와 대조했다.
- [ ] 수정 파일: `crates/clearra-core-domain/src/objective`, `crates/clearra-piece-registry/src/registry/piece_registry.rs`, 구현 내용: **S03 — 작은 도메인 가족 하나씩 응집도를 정리한다.** 선행: F03, F04. 완료: 관련 enum·정책·impl의 소유권과 공개 import 호환성을 보존한다. 각 가족을 별도 변경으로 제출하며 성능 개선으로 보고하지 않는다.
- [ ] 수정 파일: `crates/clearra-app/src/request.rs`, `crates/clearra-app/src/response.rs`, `crates/clearra-app/src/run_request.rs`, `crates/clearra-app/src/search_backend_warmup.rs`, 구현 내용: **S04 — 얇은 전달·재수출 파일의 통합 여부를 판정한다.** 선행: F03, H01. 완료: 실제 외부 소비자와 공개 경로를 보존하며 불필요한 탐색·이동 비용만 줄인다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/result.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs`, 구현 내용: **S05 — 대형 모듈의 연산·증거·수명을 정리한다.** 선행: M03, G12, O08. 완료: relation compiler, oracle, 실행 수명, 결과 조립의 실제 owner가 명확하다. 분리의 재계산·복사·보유 비용이 크면 함께 유지하거나 융합하며 새 파일 생성을 완료 조건으로 삼지 않는다. 순수 파일 이동과 알고리즘 수정은 분리한다.
- [ ] 수정 파일: `crates/clearra-coverage/src/cover/exact_minimum_cover.rs`, `crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs`, 구현 내용: **S06 — 다른 대형 모듈의 후속 분리 경계를 조사한다.** 선행: F05. 완료: 현재 핵심 탐색 작업을 막지 않는 별도 책임·소비자 지도를 남긴다. 조사 후 실제 분리는 독립된 후속 구현 범위로 진행한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/mod.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu`, 구현 내용: **S07 — 안정된 공통 CPU 모듈의 이름·재수출을 정리한다.** 선행: R08, G15, H05. 완료: host 이름과 알고리즘 소유권이 혼동되지 않고 public API 호환 계획에 따라 순수 이동 변경을 완료한다.

### V. 정확성·성능·최종 gate — 6개

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_domain.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, 구현 내용: **V01 — ILC·APDP·support/clear를 독립 gate로 비교한다.** 선행: support/clear는 I02·G02·G03, instantiation 재사용은 I03, 증분 APDP는 A04. 완료: 준비된 범위마다 full parent/domain·후보 집합과 반례를 검증하며 다른 범위 구현을 기다리지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_reference_tests.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_dependency_differential_tests.rs`, 구현 내용: **V02 — 지원 미노·프로필의 관계를 독립 물리 oracle로 검증한다.** 선행: 기본 규칙은 G06·G07·G14, 새 kick/분리 규칙은 G08·G09. 완료: 준비된 범위마다 도달/불가 양쪽·kick·먼 통로·공동 차단·같은 kind 사례를 비교하며 적용 범위를 명시한다.
- [ ] 수정 파일: `crates/clearra-core-ffi/src/native`, `crates/clearra-invariant-tests/src`, 구현 내용: **V03 — C/Rust parity를 제품 기능별로 완료한다.** 선행: F08 및 해당 기능의 R03·R04·R05·R06. 완료: 준비된 기능부터 identity·coverage·CountAll·hold·terminal evidence·취소/부분 결과를 비교한다. R08은 통과한 기능만 전환하며 전체 지원 기능 합집합은 V06에서 확인한다.
- [ ] 수정 파일: `crates/clearra-core-executor/examples/placement_graph_bench.rs`, 구현 내용: **V04 — 동일 계약의 단계별 작은·중간 A/B를 수행한다.** 선행: F06, M01, M02, V01, V02, Q03. 완료: 정확성 일치 뒤 관계 구성 연산·ILC/APDP 재계산·메모리·시간을 분리하고 변화 요인 하나씩 비교한다.
- [ ] 수정 파일: `crates/clearra-core-executor/examples/placement_graph_bench.rs`, `docs/research`, 구현 내용: **V05 — P7P4 완료 실행과 규모 증가 검증을 수행한다.** 선행: V04, Q05. 완료: Auto 워커·같은 resource 정책으로 전체 결과를 검산하고 10~11미노 구간의 생성 관계·물리 호출·state·시간·peak를 비교한다. 실패 실행 시간을 speedup으로 쓰지 않는다.
- [ ] 수정 파일: `scripts/architecture`, `.github/workflows`, `docs/architecture-validation.md`, 구현 내용: **V06 — 통합된 소스의 배포 gate와 최종 근거를 확정한다.** 선행: R12, H05, V05. 완료: 현재 요구 CI·제품·WASM/native gate를 실제 실행하고 C 전제 교체 때문에 무관한 검증을 약화하지 않는다. main 반영은 이 검증 후 정확 SHA fast-forward로만 한다.

## 12. 병렬 실행과 통합 규칙

### 12.1 작업 흐름

다음 그림은 작업군의 흐름이다. 세부 선행조건은 11절을 따른다. 하나의 가지가 끝날 때까지 모든 다른 가지가 기다리는 일괄 단계제가 아니다.

```mermaid
flowchart TD
    F["공통 계약과 소유권"] --> O["기존 객체 공통화"]
    F --> M["종료와 메모리 진단"]
    F --> S["파일 및 검증 문서 정리"]
    O --> G["관계와 사건 규칙"]
    O --> I["ILC와 APDP 재사용"]
    O --> R["Rust 기능별 전환"]
    G --> Q["확률 및 목표 결합"]
    I --> Q
    R --> H["공통 호스트 경로"]
    Q --> V["정확성 및 규모 검증"]
    H --> V
    M --> V
    S --> C["중복 경로 퇴역"]
    V --> C
```

화살표는 **소비할 범위의 계약**을 뜻한다. O 전체가 끝나야 G/I/R을 시작한다는 의미가 아니다. 예를 들어 O03의 순수 행사상 계약이 정해지면 I01을 시작할 수 있으며, support/clear 검증과 O01의 kick 정규화 작업도 독립된 파일에서 진행할 수 있다. S02의 작업 규약 수정은 소스 집계 F05나 알고리즘 구현을 기다리지 않는다.

### 12.2 3명 작업자와 1명 통합자 기준 배치

| 준비 상태 | 작업자 A | 작업자 B | 작업자 C | 통합자 |
| --- | --- | --- | --- | --- |
| 조사·계약 작성 | R01·R02의 backend/ABI 지도 | F05·S02의 소스·검증 문서 | F07·M01의 종료 진단 | F01·F02·F03, 서로 다른 계약 조정 |
| 공통 인터페이스 확정 | O01·O02의 규칙 원천 | O03·I01·I02·O04의 frame/catalog | M02 및 R02의 기능별 oracle 준비 | 공개 API·Cargo·모듈 선언 편집 |
| 기본 관계 준비 | G02·G03의 support/clear | A02·A03·A04의 APDP 부모 전파 | G05·G06의 ordered kick/진입 규칙 | G01, shared-file 변경 연결, V01/V02 범위별 검증 |
| 기본 관계 연결 후 | G08·G09·G14의 일반화 | I03·I05·I06·G13의 재사용 | R03~R06의 제품 격차 해소 | G15, 공통 workspace/result 연결 |
| 의미 계약 안정 후 | G10·G11의 목표 조건 생산 | Q01~Q03·Q05의 공급 결합 | H01~H04의 host adapter | G12·M03~M05, 기능별 R08 전환 |
| 증거 확보 후 | S03·S04의 작은 가족 정리 | I04·A06·A07·Q04의 독립 연구 | 필요한 ABI·배포 연결 | 순차 벤치마크, S05·S07, R10~R12, V06 |

이 표는 담당 분야의 예시다. 선행조건이 준비되지 않았으면 해당 작업자는 독립 조사·oracle 설계·다른 ready 작업을 먼저 한다. 특히 같은 파일에 도착한 두 작업을 병렬 적용하지 않는다. 별도 작업 공간을 사용해도 공통 인터페이스 변경의 통합은 순차로 한다.

### 12.3 파일 편집권

| 충돌 지점 | 편집 책임 | 병렬로 할 수 있는 일 |
| --- | --- | --- |
| `Cargo.toml`, 공개 `lib.rs`, `backend/mod.rs`, `wasm_cpu/mod.rs` | 통합자 한 명 | 각 owner 내부 구현·소비자 조사 |
| `catalog.rs`, `placement_conditions.rs` | ILC/catalog 담당; 관계 담당의 요구를 계약으로 받음 | APDP index·물리 규칙의 독립 구현 |
| `geometry.rs`, `geometry_domain.rs` | 해당 단계의 domain/family 담당 | 관계 증명·제품 adapter |
| `reachability.rs`, `reachability_independence.rs` | movement 담당 | support/clear·supply·manager |
| `buildup.rs` | verification 통합 담당 | 규칙 생산자·독립 oracle·목표 policy 생산자 |
| `result.rs` | 결과·수명 담당 | protocol 소비자·기하 알고리즘 |
| `app_services.rs` | app 통합 담당 | CLI/Discord/Web의 각 adapter |
| C 삭제·공개 rename·feature 전환 | 통합자 순차 | 다른 안정된 모듈의 조사·작은 변경 |

작업자는 담당 밖의 shared 파일을 임의로 수정하지 않고 필요한 최소 API 변경을 통합자에게 전달한다. 계약만 같으면 테스트·실험의 설계는 병렬화할 수 있다. **큰 메모리 실행은 작업 간 코드 의존성이 없어도 병렬화하지 않는다.**

### 12.4 불필요한 선행조건을 만들지 않는 예

- M01은 Rust 코어 이전이나 새 관계 알고리즘을 기다릴 필요가 없다.
- 기존 static APDP의 활성 손익 검토는 temporal APDP 연구를 기다릴 필요가 없다.
- support/clear와 이미 증명된 진입 규칙으로 완성되는 관계는 모든 kick 반례의 일반화를 기다릴 필요가 없다.
- 목표 정책 생산자는 G10이 확정되면 물리 그래프 소비자와 다른 파일에서 병렬 구현할 수 있다.
- 기능별 Rust parity를 통과하면 해당 기능을 전환할 수 있다. 모든 UI/ABI 가족이 동시에 준비되어야만 시작하는 구조를 만들지 않는다.
- 관계 알고리즘 개선은 전체 파일 이동이나 모든 C 삭제를 기다리지 않는다.

### 12.5 작은 작업의 실행·인계 절차

88개 기본 작업은 담당 범위이며 한 번의 큰 변경 단위가 아니다. 담당자는 각 작업을 아래 순서로 잘라 진행한다. 이 절의 단계마다 새 파일·crate·상태 타입을 만들지 않는다.

1. **계약 작성:** OP 연산군, 실제 입력/출력, 필요조건/충분조건/정확 결과의 강도, 지원 보드·미노·프로필을 적는다. key 밖에서 owner가 고정하는 값과 변경 시 무효화할 값도 지정한다. 이 단계는 읽기 전용 조사로 병렬 진행할 수 있다.
2. **소비자 한 곳 선택:** 가장 작은 실제 소비자와 비교 oracle을 정한다. 기존 결과를 전달할 수 있는지 먼저 확인한다. 전 저장소 rename이나 추상 인터페이스를 먼저 만들지 않는다.
3. **한 경로 연결:** 기존 계산 결과를 그 소비자에서 사용하게 하고, 남아 있는 중복 계산과 fallback을 명시한다. stale owner·다른 D·다른 profile·부분 계산을 정확 결과로 재사용하지 않는 사례를 해당 범위에서 검증한다.
4. **동일 계약의 소비자 확대:** 같은 의미인 소비자만 순차 연결하고, compact/extended 또는 Boolean/evidence 차이는 별도 하위 범위로 남긴다. C와 Rust의 이름이 같다는 이유로 연결하지 않는다.
5. **중복 퇴역:** 마지막 실제 소비자가 사라진 계산·wrapper만 제거한다. 필요한 독립 oracle은 보존한다. 파일 이동은 동작 변경과 분리한다.
6. **인계와 통합:** 변경한 경로, 감소시킨 연산, 기존/새 owner, 실제 검증 결과, 미지원·미완성 범위를 다음 소비자 담당자에게 넘긴다. 검사 문구 존재나 체크박스 수로 완료를 판정하지 않는다. 큰 실행은 통합자가 준비된 범위의 gate를 확인한 뒤 순차 수행한다.

다음은 동시에 배정할 수 있는 구체적인 작업 묶음이다. 계약 정리와 oracle 설계는 해당 구현 파일의 편집권이 없어도 진행할 수 있다. 표의 준비 조건은 각 묶음의 연결 단계에 적용하고, 최종 완료 조건은 11절을 유지한다.

| 독립 묶음 | 시작할 작은 작업 | 준비되어야 할 계약 | 다음 소비자와 인계물 |
| --- | --- | --- | --- |
| 행 프레임 | O03의 순수 사상과 adapter 범위를 대조 | F02·F03 | I01/G03: 행 순서·D 누적·compact/extended 범위 |
| 규칙 정규화 | O01/O02의 세 정규화 소비자와 ID 인코딩 대조 | F03 | G05/O07: ordered delta·좌표·profile 변경 조건 |
| 정확 realization | I01에서 약한 domain 소비자와 exact 소비자 분리 후 I02의 한 경로 연결 | O03 | G02/I03: owner별 `(skeleton,D)`의 정확 alternative view |
| 앞단 투영 재사용 | I05에서 같은 shape/row 조건이 다시 계산되는 호출군 선정 | I02 | catalog 담당: 의미가 완전한 공유 입력과 제거한 projection 계산 |
| APDP 증분 | A02의 full parent 대안 확인, A03의 영향받은 parent 갱신, A04의 rollback 비교 | F02·O04 | A05/G13: 변화한 cell/partial과 보존되는 parent 대안 |
| Support와 clear | G02의 supporter OR와 G03의 row contributor를 각각 준비 | G01·I02·O04; clear는 O03 | G15/G13: 직접 영향과 clear 간접 영향이 분리된 관계 |
| Movement·CEGIS | G05/G06의 전제와 positive proof, G14의 반례 최소화 | F08·G01·O01의 해당 규칙 계약 | G15/V02: 범용/미노/프로필별 적용 범위와 residual fallback |
| 공급 product | Q01의 관계 언어 입력 및 Q02의 canonical 공급 전이 | G01·F02 | Q03: occurrence·hold 다중성과 request/epoch namespace |
| 소유권·증거 | O05/O06의 수명 지도와 M02 계측 후 M03의 한 결과 모드 | F03·F07, 해당 compiled 계약 | M04/R06: 공유/worker/후보/결과와 incomplete 처리 |
| 제품·ABI | R01/R02에서 기능 가족 하나의 소비자와 지원 범위 확인 | F01·F02 | R03~R06: 재사용/미연결/미구현을 구분한 실제 요청 fixture |

각 묶음은 한 worker가 여러 파일을 소유하거나 여러 단계가 한 파일 안에서 끝날 수 있다. 반대로 한 작업 ID가 행 사상 계약, Boolean 소비자, evidence 소비자의 별도 제출로 나뉠 수 있다. 통합자는 `catalog.rs`·`buildup.rs`·`result.rs` 등의 공통 파일에 들어오는 변경을 12.3절 편집권에 따라 순차 연결한다. **병렬화의 단위는 파일 개수가 아니라 서로 독립적인 계산 계약과 변경 범위다.**

## 13. 정확성 및 성능 검증 설계

### 13.1 정확성 gate

| 검증 축 | 포함할 반례·검사 | 통과 의미 |
| --- | --- | --- |
| support | Z의 L 또는 S, 초기 받침, 바닥, 미래 parent, clear 뒤 받침 변경 | OR와 미래 대안의 완전성 유지 |
| 이동 독립성 | 먼 통로, 두 blocker의 공동 차단, 유효 범위 밖 변경 | 제외한 변수에 대해 보편적 무영향 |
| ordered kick | 앞 후보 성공/실패가 뒤 선택에 미치는 영향, 상승·180 | 프로필의 first-success 의미 유지 |
| clear 사건 | 동시 여러 줄, 삭제 후 잔존 셀, 원래 행↔물리 행 | 즉시 atomic clear와 frame 일치 |
| ILC | 약한 필요조건과 exact 실현의 차이, 자기 행 삭제, extended 한도 | domain과 realization 집합 일치 |
| APDP | parent 대안의 상관관계, parent 제거/복구, static 범위 밖 | full domain과 증분 domain 동등 |
| 같은 미노 | 여러 같은 kind 배치, 현재와 hold가 같은 kind | concrete identity와 선택 다중성 유지 |
| 목표 조건 | 0/4/PC, T-only/non-T, mini/full spin, last rotation·kick | save/chain/score와 terminal 증거 구별 |
| 확률 | 여러 타일링이 같은 큐 성공, nonuniform weight, 긴 공급 | 성공 큐 합집합과 분모·가중치 보존 |
| 결과 모드 | CountUnique/CountAll, 확률만, 대표 trace, 모든 증거 | 필요한 결과만 만들되 계약 누락 없음 |
| 자원 | 취소, 예산 초과, allocation 실패, 외부 종료 | Impossible과 Incomplete 구별 |
| host | 동일 canonical request를 native/WASM/CLI/Discord에서 실행 | 전달·분산 차이가 결과 의미를 바꾸지 않음 |

필요한 수학적 증명은 테스트로 대체하지 않는다. 작은 전수·차등 검사, 독립 물리 oracle, 제품 전체 비교는 각기 다른 근거로 보존한다. 해 수만 같거나 hash만 같다는 이유로 모든 의미가 같다고 선언하지 않는다. 비교 가능한 범위에서는 canonical identity·coverage·label의 실제 값을 비교하고, 큰 실행의 digest·개수·completeness는 별도로 기록한다.

### 13.2 구조적 연산 감소 지표

| 단계 | 먼저 볼 지표 | 해석상 주의 |
| --- | --- | --- |
| ILC | 새 projection, exact-domain 유도, raw instantiation, frame 재사용 | 새 캐시 hit 수만으로 개선을 주장하지 않음 |
| APDP | full domain scan, 바뀐 parent, 재검사 cell/partial, rollback | 미활성 요청의 0회와 실제 가속을 구분 |
| 관계 구성 | 생성 전 제외 쌍, 새 규칙 형태, 새 fragment, 증분 재결합 | 단순히 셀을 덜 봤다는 수치와 관계 수를 구분 |
| 물리 검증 | 관계를 만들기 위한 physical 호출, residual BuildUp 후보·노드 | 전처리 내부의 도달성 비용도 합산 |
| 확률 | materialized queue/prefix, 공유 언어 상태, union·weight 전이 | topological order 수를 성공 큐로 착각하지 않음 |
| 메모리 | shared/worker/live candidate/retained result/transport | 추정 보유량과 측정 RSS를 구분 |
| 제품 | core·초기화·직렬화·전송·merge·출력 시간 | 출력 모드가 다르면 같은 비교가 아님 |

핵심 성과는 **같은 결과를 내기 위해 새로 생성한 관계·상태·증거가 얼마나 줄었는가**다. 시간·메모리까지 함께 개선되어야 채택한다. 관계 구축 비용이 줄었어도 표현식 또는 공급 결합이 더 커져 총비용이 증가하면 다음 규칙을 재설계하거나 해당 범위의 적용을 보류한다.

### 13.3 큰 탐색의 비교 계약

빈 10×4, 4L PC, 10개 배치/11개 공급 P7P4, 빈 hold, Jstris180, CountUnique, full-future-oracle를 기준 사례로 유지한다. 기존 검산 기준은 456,459 canonical 해와 전체 성공 큐 4,233,600개다. 이 숫자는 기존 runner/report의 기준이며 이번 기획 턴의 새 실행 결과가 아니다. 개별 해 확률까지 비교하려면 해당 결과 모드를 실제로 켜서 별도 검산해야 한다. [S5]

가능하면 동일 바이너리의 한 변화 축만 A/B로 바꾼다. C와 Rust의 빌드 계약이 다르면 그 차이를 명시하고 결과·출력·워커 조건을 맞춘다. cold/warm을 섞지 않고 순차 교차 실행하여 변동을 기록한다. 평균 하나뿐 아니라 각 완료 실행과 중앙값·산포·peak를 남긴다.

이후 7~11개 배치 규모에서 공급 길이·초기 필드·출력량을 함께 명시해 비교한다. 실행 시간이 늘어난 원인을 필드/타일링 수 증가, 관계 생성 증가, residual 물리 상태 증가, 결과량 증가로 나눈다. 서로 다른 문제 크기 사이의 시간 비율만으로 알고리즘 개선을 증명하지 않는다.

환경에서 자동 감지한 논리 워커 수를 사용한다. 이번에 알려진 Cloud 9와 사용자가 언급한 12를 비교할 때는 환경 차이를 명시한다. 종료 신호·resource 정보가 충분하지 않으면 반복 실행으로 시간을 채우지 않고 원인 관측을 먼저 보완한다. 미완료 실행 시간은 완료 시간·speedup이 아니다.

## 14. 단계별 완료 판정

| 단계 | 사용자에게 제공할 결과 | 완료 기준 |
| --- | --- | --- |
| MS0: 계약 정리 | 현재/목표 구조, 소유자·기능 격차·검증 기준 | 기존 코어를 무엇에 재사용할지 확정; 구현 중복 금지 |
| MS1: 기본 관계 | 모든 미노의 support OR·clear 사건·incidence 기반 제외 | 검증된 진입 범위에서 관계 구성 비용 감소; 미완성 범위 명시 |
| MS2: 일반 이동 규칙 | ordered kick·먼 통로·공동 차단 반례와 일반화 | 프로필별 완전 관계 적용 범위 확대; 물리 구성 호출 감소 |
| MS3: 앞단 재사용 | ILC/domain 단일 소유·증분 APDP·초기 family 조각 | 활성 경로에서 반복 평가 감소, 후보 집합 보존 |
| MS4: Rust 공통 코어 | 기능별 native/WASM 서비스와 호환 경계 | 기존 Rust 구현 기반 parity; host의 알고리즘 중복 없음 |
| MS5: 그래프와 확률 | 목표 조건·같은 kind·hold·weighted union의 결합 | 확률 전용 요청에서 불필요한 큐·물리 prefix 감소 |
| MS6: 규모와 제품 | 완료 P7P4·10~11미노 비교·host 분석 | 결과 동일, 시간·메모리·잔여 병목을 실제 완료 실행으로 설명 |
| MS7: 퇴역과 배포 | 불필요한 C·wrapper·파일 정리, 문서와 gate 일치 | 요구 CI·제품 gate 통과; 정확 SHA를 근거로 반영 |

MS2·MS3·MS4는 병렬 진행 가능하다. MS1의 개선을 모든 연구의 끝까지 보류하지 않는다. 단, 일부 타일링을 exact하게 처리하는 단계와 전체 요청 범위의 관계가 완성된 단계는 분명히 구분한다. **전체 관계 알고리즘의 완성 선언에는 해당 범위의 의미 동등성과 남은 구성용 물리 탐색의 공개가 필요하다.**

현재 O 구현은 검증된 특수 규칙과 oracle 자산으로 유지하는 계획이다. 새 관계가 이를 자연스럽게 포함하는지 검증하기 전에는 철회하지 않는다. 현재 experimental 경로의 default나 제품 설정도 이 문서 작성만으로 바꾸지 않는다.

다음 구현을 시작할 때의 최소 묶음은 공통 계약 확정, M01 종료 진단, O01/O03의 의미 원천 정리, I01/I02/O04의 기존 catalog 재사용, G02/G03/G04의 기본 관계다. 이들이 첫 성과를 만들고, APDP 증분 전파와 C/Rust parity는 준비된 계약부터 독립적으로 진행한다.

## 15. 현재 결정하지 않는 항목

- 기호적 ILC domain이 모든 extended/custom 범위에서 작게 표현되는지.
- 모든 kick profile에서 관계를 동일한 작은 표현으로 완성할 수 있는지.
- family 경계 summary가 실제 후보별 구성보다 충분히 작은지.
- APDP의 빈 필드 활성화가 총 계산량을 줄이는지.
- 외부 C ABI 소비자가 어느 심볼을 실제로 필요로 하는지.
- 현재 SIGKILL이 커널 OOM, 외부 환경 제한, 다른 종료 원인 중 무엇인지.
- 실제 배포된 GUI·Discord·CLI의 차이가 backend, 출력 계약, worker 또는 host overhead 중 어디서 발생하는지.

위 항목은 모르는 채로 구현 전제를 굳히지 않는다. 각 항목은 조사·증명·측정으로 닫을 담당 작업이 있으며, 독립된 다른 작업의 진행까지 막지는 않는다.

## 16. 근거와 재현 범위

초안 소스 근거와 파일 수 집계는 [초안 기준 커밋](https://github.com/daejunnom/Clearra/tree/caacfae0c9958949858bb846f2bb52eb5fcbb50d)을 대상으로 한다. 7.3~7.4절의 내부 연산 추가 감사는 [추가 감사 기준 커밋](https://github.com/daejunnom/Clearra/tree/45cbf15eec6a6c8821946756807f90703466dff6)을 읽었다. 아래 파일 링크는 현재 저장소에서 여는 상대 링크다. 병행 CEGIS 제품 변경과 실행 결과는 5.7절의 별도 보고서가 담당하며, 이번 문서 감사에서 실행한 빌드·테스트·벤치마크로 취급하지 않는다.

| ID | 확인한 근거 |
| --- | --- |
| S1 | [app 서비스](../../crates/clearra-app/src/app_services.rs)의 `AppCoreExecutorBackend`, `execute_search_backend_with_control`, `execute_build_coverage_with_control`; [CLI 선택](../../crates/clearra-cli/src/cli_routing.rs)의 `product_app_context`; [C source manifest](../../core-c/cmake/source_manifest.cmake) |
| S2 | [GeometryCatalog](../../crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs), [PlacementConditionCatalog](../../crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs), [APDP index](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_apdp.rs), [family](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_family.rs); [일반 reachability](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability.rs), [extended](../../crates/clearra-core-executor/src/backend/wasm_cpu/extended_reachability.rs), [finesse](../../crates/clearra-finesse/src/lib.rs)의 정규화 함수 |
| S3 | [Rust 계약 facade](../../crates/clearra-core-ffi/src/memory/core_context.rs), [native RAII](../../crates/clearra-core-ffi/src/memory/native_core_context.rs), [C allocator](../../core-c/src/memory/clr_allocators.c), [scope](../../core-c/src/memory/clr_scope.c) |
| S4 | [geometry.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs)의 `FamilyCompiler::advance`, `GeometryCompletionOracle`; [geometry_domain.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_domain.rs)의 `compile`, `compile_minimum`, APDP complete-domain 판단 |
| S5 | [기존 구현·미완료 P7P4 보고서](static-movement-influence-2026-11-20.md), [이전 graph probability 보고서](graph-only-probability-2026-11-20.md), [manager runtime](../../tools/clearra-manage/src/runtime.rs)의 exit status 처리 |
| S6 | [B2B-save](../../crates/clearra-scoring/src/b2b_preservation.rs), [B2B chain](../../crates/clearra-scoring/src/event/score_event.rs), [기존 wrapper](../../crates/clearra-postprocess/src/score_batch/b2b_execution_filter.rs), [목표 계약](../../crates/clearra-objectives/src/policy/execution_constraint_policy.rs), [물리 scoring evidence](../../crates/clearra-replay/src/scoring_execution.rs) |
| S7 | [workspace 설정](../../Cargo.toml), [executor 설정](../../crates/clearra-core-executor/Cargo.toml), [FFI 설정](../../crates/clearra-core-ffi/Cargo.toml) |
| S8 | [Discord 직접 실행](../../apps/clearra-discord-bot/src/clearra/direct-executor.mjs), [job runner](../../apps/clearra-discord-bot/src/job-service/runner.mjs), [Web worker](../../apps/clearra-web/src/workers/DistributedWasmJobRunner.ts), [CLI routing](../../crates/clearra-cli/src/cli_routing.rs) |
| S9 | [작업 규약](../../AGENTS.md), [검증 문서](../architecture-validation.md), [PowerShell 문법 검사](../../scripts/architecture/validate_srp_policy.ps1), [대상 파일 열거](../../scripts/architecture/validate_file_size.ps1), [의존성 경계](../dependency-boundary.md). 역사적인 스크립트 이름을 SRP gate로 해석하지 않음 |
| S10 | [공통 row frame](../../crates/clearra-core-executor/src/row_frame.rs), [범위를 좁힌 wrapper](../../crates/clearra-core-executor/src/conditioned_local_relation.rs), [공통 projection](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_projection.rs) |
| S11 | 기준 SHA의 `git ls-files` 경로 목록과 원본 줄 수 집계. `tools/vendor/` 제외 자체 Rust 2,264개; 계획용 제품 분류의 제외 규칙은 2.4절에 명시. [대형 cover](../../crates/clearra-coverage/src/cover/exact_minimum_cover.rs), [build probability](../../crates/clearra-core-executor/src/backend/wasm_cpu/build_probability.rs), [result](../../crates/clearra-core-executor/src/backend/wasm_cpu/result.rs), [desktop bridge](../../crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs) |
| S12 | [C target-frame projection](../../core-c/src/packing/target_frame_projection.c), [C exact realization](../../core-c/src/packing/geometry_realization_domain.c), [Rust catalog](../../crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs), [Rust ILC 필요조건](../../crates/clearra-core-executor/src/backend/wasm_cpu/inverse_projection.rs), [BuildUp 행 연산](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs). OP01/03/04의 범위·명제 강도·fallback 근거 |
| S13 | [C first-success](../../core-c/src/reachability/kick_first_success.c), [Rust profile 연결](../../crates/clearra-core-executor/src/backend/wasm_cpu/kick_profiles.rs), [Rust reachability](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability.rs)의 정규화·`SharedReachabilityTemplates`·cache 권한/무효화; extended/finesse 대응은 S2 |
| S14 | [C grounding](../../core-c/src/buildup/grounded_filter.c), [C realization domain propagation](../../core-c/src/buildup/realization_domain_propagation.c), [C APDP](../../core-c/src/apdp/geometry_apdp.c), [C Hall](../../core-c/src/pruning/geometry_parent_hall_bound.c), [Rust realization feasibility](../../crates/clearra-core-executor/src/backend/wasm_cpu/realization_feasibility.rs), [Rust APDP domain](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_domain.rs). OP05/06/11의 필요조건·완전 도메인·prepass 비활성·stale generation 근거 |
| S15 | [C residual memo](../../core-c/src/packing/geometry_residual_memo.h), [C completion memo](../../core-c/src/buildup/buildup_completion_memo.h), [C physical transition key](../../core-c/src/buildup/buildup_geometry_transition_cache.h), [C workspace 무효화](../../core-c/src/buildup/buildup_workspace.c), [Rust family/completion](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs), [점수 증거 재생성](../../crates/clearra-core-executor/src/backend/wasm_cpu/result.rs)의 `prepare_exact_scoring_execution_batch` |
| S16 | [canonical supply automaton](../../crates/clearra-supply/src/execution_automaton/supply_execution_automaton.rs), [hold alias](../../crates/clearra-supply/src/hold_automaton/hold_automaton.rs), [standard-bag coverage](../../crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage.rs), [product memo key](../../crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_product_memo.rs). OP10의 일반/특수 공급 범위와 request/epoch 소유권 근거 |
| S17 | [ExecutionControl](../../crates/clearra-core-domain/src/execution_cancellation.rs), [native scope](../../crates/clearra-core-ffi/src/memory/native_scope.rs), [native context](../../crates/clearra-core-ffi/src/memory/native_core_context.rs). C allocator 및 optional proof 근거는 S3/S14 |
| S18 | [C generic BuildUp 지원 범위](../../crates/clearra-core-ffi/src/problem/generic_buildup.rs), [서비스의 실제 지원·미연결 분기](../../crates/clearra-app/src/app_services.rs). OP12의 C/Rust 기능 격차 분류 근거 |
| E1 | [Google 공식 Rust Style Guide](https://google.github.io/styleguide/rustguide.html), 2026-11-20 열람. 일반 규칙과 `[chrome]` 등 특정 코드베이스 규칙을 구분하여 적용 |

기준 SHA의 추적 C/H 285개는 `src/` 176개, `include/` 25개, `tests/` 83개, `tools/` 1개다. 다운로드·시스템 라이브러리의 native 코드 수는 이 집계에 포함하지 않았다. 파일·줄 수 집계는 성능 측정이 아니다.

문서는 C/Rust 경로, ILC/APDP, 파일·SRP의 독립 검토에 12개 내부 연산군의 소스 감사를 보강했다. 감사 결과는 지원 범위와 명제 강도를 설명하는 근거이며 성능 parity나 속도·메모리 향상을 실행으로 입증한 결과가 아니다. SRP 문구·길이·marker gate의 복구는 후속 구현 범위에도 포함하지 않는다.

[S1]: ../../crates/clearra-app/src/app_services.rs
[S2]: ../../crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs
[S3]: ../../crates/clearra-core-ffi/src/memory/core_context.rs
[S4]: ../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs
[S5]: static-movement-influence-2026-11-20.md
[S6]: ../../crates/clearra-scoring/src/b2b_preservation.rs
[S7]: ../../Cargo.toml
[S8]: ../../apps/clearra-web/src/workers/DistributedWasmJobRunner.ts
[S9]: ../architecture-validation.md
[S10]: ../../crates/clearra-core-executor/src/row_frame.rs
[S11]: https://github.com/daejunnom/Clearra/tree/caacfae0c9958949858bb846f2bb52eb5fcbb50d
[S12]: ../../crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs
[S13]: ../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability.rs
[S14]: ../../crates/clearra-core-executor/src/backend/wasm_cpu/realization_feasibility.rs
[S15]: ../../core-c/src/buildup/buildup_geometry_transition_cache.h
[S16]: ../../crates/clearra-supply/src/execution_automaton/supply_execution_automaton.rs
[S17]: ../../crates/clearra-core-domain/src/execution_cancellation.rs
[S18]: ../../crates/clearra-app/src/app_services.rs
[E1]: https://google.github.io/styleguide/rustguide.html
