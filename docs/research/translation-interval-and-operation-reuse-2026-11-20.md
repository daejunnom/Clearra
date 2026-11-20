# 모든 표준 미노의 무회전 진입 증명과 연산 단위 재사용 — 2026-11-20

## 1. 이번 변경의 범위

이 보고서는 `prototype/placement-dependency-20261120`의 `45cbf15eec6a6c8821946756807f90703466dff6`을 기준으로 한 후속 구현이다. 날짜는 한국 시간 기준이다. 전체 Rust 전환이나 완전한 의존성 관계 컴파일러의 완료 보고가 아니다.

이번에 진행한 작업은 다음 세 가지다.

1. **SRP를 작업·리뷰 규약으로 수정했다.** 1,000줄 제한, 필수 설명 문구, 파일·타입·crate 개수와 형태에 의한 정적 합격 기준을 제거했다. 연산 분리가 반복 계산·복사·할당·장기 보유를 늘리면 상태와 연산을 함께 둘 수 있다. 정확성·완전성·실제 의존성·ABI·unsafe 계약은 유지한다.
2. **C→Rust 범위를 내부 연산 12개 군으로 다시 조사했다.** 기존 Rust 커널, 같은 의미의 중복 계산, 다른 적용 범위, 서비스 API 미연결을 구분했다. 상세 계약과 병렬 작업은 [통합 로드맵 7.3·7.4·12.5절](rust-core-and-relation-roadmap-2026-11-20.md)에 있다. 새 파일을 만들거나 C 파일을 일대일 번역하는 수로 작업량을 산정하지 않는다.
3. **CEGIS 방식의 일반화 한 단계를 구현했다.** 기존 모든 미노의 수직 진입 증명을 좌우·하강으로 들어가는 경로까지 확장했다. 한 개의 고정 행 프레임과 점유 구간 전체에 대해 성공을 증명하며, 개별 중간 보드나 pose 방문 집합을 만들지 않는다. 실패는 Unknown이고 기존 정확한 경로로 돌아간다.

사용자가 표현한 CIGES는 기존 대화의 CEGIS식 수동 반례 수집·규칙 일반화 작업으로 해석했다. 자동 합성 엔진을 추가하거나 검증기에 모든 필드를 넣어 새 규칙을 학습시킨 것은 아니다.

**결과 판단:** 범용 양성 증명과 실제 제품 연결은 검증했지만, 이번에 측정한 비-O 제품 사례에서 전체 배치 그래프나 primitive 상태는 줄지 않았다. 단독 양성 증명은 얻어도 이동·줄 삭제가 만드는 필요한 선행 간선을 아직 추론하지 못하는 비용 반례를 확보했다. 따라서 새 축은 기본값을 끈 실험 상태로 유지하며, 이를 전체 검색 속도 개선으로 보고하지 않는다.

## 2. 기존 구현과 이번 일반화의 차이

기존 `placement_poset::compile_with_guard`는 이미 모든 표준 미노에 대해 수직 진입 corridor가 최대 점유 보드와 충돌하지 않는 경우를 증명한다. 또한 접지는 `placement_is_grounded` 등에서 이미 확인한다. 이번 변경은 기존 접지 검사나 수직 진입 검사를 새로운 성능 개선으로 계산하지 않았다. 새 구간 API는 자신의 입력 계약을 위해 접지를 다시 확인한다.

수직 진입이 막힌 경우의 다음 단계를 추가했다. 예를 들어 지붕 아래의 T 배치에 직접 내려갈 수 없더라도, 옆 입구를 통해 내려온 다음 좌우로 이동할 수 있다. 이를 매번 BFS로 확인하거나 먼저 얻은 경로 캐시에 의존하지 않고 행별 허용 anchor 집합의 관계로 판정한다.

| 구분 | 사용 범위 | 성공의 의미 | 실패의 의미 |
|---|---|---|---|
| 기존 수직 진입 증명 | 모든 표준 미노 | 점유 구간의 모든 보드에서 해당 lock 가능 | 다른 증명 또는 정확한 검증 필요 |
| **이번 고정 방향 좌우·하강 증명** | 모든 표준 미노, 현재 연결된 이동 계약 | 점유 구간의 모든 보드에서 해당 lock 가능 | **Unknown**. 회전·킥 경로가 남을 수 있음 |
| 기존 O의 단일 보드 exact 판정 | 실제 정규화된 모든 O 킥이 위치를 바꾸지 않는 프로필 | 해당 보드에서 lock 가능 | 해당 Boolean 이동 계약 안에서 lock 불가 |
| 기존 O의 구간 증명 | 위 O 계약과 동일 행 프레임 | 점유 구간 전체에서 lock 가능 | 구간 증명 실패이므로 Unknown |
| 기존 관측 기반 양성 증명 | 이미 유효한 증거가 있는 구간 | 증거의 조건 안에서 lock 가능 | 증거 없음; 불가능을 뜻하지 않음 |

범용 양성 규칙과 O/profile 전용 exact 규칙은 서로 다른 API와 결과 타입으로 남겼다. **NoKick도 회전을 허용하므로 일반 규칙의 실패를 negative 판정으로 승격할 수 없다.**

## 3. 수학적 계약

### 3.1 고정해야 하는 전제

- 표준 registry의 한 미노, 한 회전 상태에 대응하는 정확한 네 셀을 사용한다.
- 좌표계·보드 크기·원래 행 삭제 집합 `D`가 고정되어 있다.
- 이동 커널은 좌우 한 칸·아래 한 칸 이동을 허용하고, 보드 위의 모든 유효 x와 회전 상태를 sky seed로 취급한다.
- 중간에 grounded인 상태에서도 이동을 계속할 수 있는 현재 Boolean lock 계약이다. 유한 lock delay, 특정 실제 spawn 지점, finesse 비용, terminal spin 증거를 주장하지 않는다.
- caller는 ILC realization의 정확한 lock mask와 현재 surviving-row frame을 전달한다. 서로 다른 프레임의 보드를 비교하지 않는다.
- 구현의 구간 admission은 현재 `LocalRelationRowFrame`에 맞춰 폭 10, 원래 높이 1~6에 한정한다. 내부 수학식이 더 일반적이라는 이유로 wide board를 허용하지 않는다.

프로필 이름 자체를 수학적 근거로 사용하지 않는다. 이번 witness는 회전 간선을 사용하지 않으므로 킥 후보의 내용과 우선순위가 그 경로를 바꿀 수 없다. 연결부는 현재 공통 sky/translation 계약으로 실행되는 builtin 프로필만 받는다. 임의 spawn·동작 규칙을 가진 custom/imported 프로필에는 이 admission을 그대로 확장하지 않는다.

### 3.2 한 보드에서의 무회전 경로

정규화된 형상을 `S`, anchor의 물리 행을 `r`, 유효한 좌우 anchor 영역을 `A`라고 하자. 보드 `B`에서 그 행의 충돌 없는 anchor 집합은 다음과 같다.

\[
F_r(B)=\{x\in A : \forall(d_x,d_y)\in S,\ (x+d_x,r+d_y)\notin B\}.
\]

실제 구현은 각 shape cell에 해당하는 점유 행을 그 셀의 `d_x`만큼 역이동한 뒤 교집합을 취한다. 보드 위의 행은 비어 있는 것으로 취급한다. 각 행에서 가능한 수평 이동은 `F_r`의 연속 구간 안에서만 일어난다.

목표 anchor `(x_t,y_t)`에서 sky 방향으로 역산한다. `CC(F,X)`를 `X∩F`에 닿는 모든 수평 연결 구간의 합집합이라고 하면,

\[
R_{y_t}=CC(F_{y_t},\{x_t\}),\qquad
R_{r+1}=CC(F_{r+1},R_r).
\]

**행에 대한 귀납 증명:** 시작 행의 수평 연결 구간은 목표에서 역방향으로 이동할 수 있는 anchor와 정확히 같다. 다음 행으로 올라갈 때 x는 바뀌지 않는다. 두 행 모두에서 유효한 anchor를 이어 붙인 후 그 행에서 수평 closure를 구하면, 역방향 좌우·상승으로 갈 수 있는 anchor 전부와 일치한다. 정방향의 좌우·하강 경로는 행을 건너뛰지 않고 같은 행으로 다시 올라가지 않으므로, 누락된 다른 행 순서는 없다. 마지막 보드 행에서 비어 있지 않은 frontier는 위의 sky seed와 연결된다.

따라서 이 재귀는 **고정 회전의 좌우·하강 부분그래프 안에서 필요충분**하다. 전체 회전·킥 그래프에는 다른 경로가 있을 수 있으므로, 외부에 제공하는 의미는 양성 충분조건이다.

수평 closure는 free 구간의 연속성도 함께 전파한다. 거리 1·2·4·8의 단계가 중간 장애물을 건너뛰지 않는다. 폭 10의 세로 I는 x=9까지 10개 anchor를 사용하므로 9비트에 한정된 옛 closure 검증을 10비트 전체로 넓혔다.

### 3.3 점유 구간 전체를 한 번에 증명

미배치 미노들의 허용 조합에 대응하는 물리 보드가 다음 구간 안에 있다고 하자.

\[
B_{\min}\subseteq B\subseteq B_{\max}.
\]

최종 lock mask를 `M`, 한 칸 내린 mask에서 자기 셀을 뺀 지지 후보를 `support(M)`이라고 하자. 다음 두 사실을 증명한다.

1. `M`이 바닥에 닿거나 `B_min ∩ support(M)`이 비어 있지 않다.
2. `B_max`에서 위의 고정 방향 이동 경로가 존재한다.

첫 사실은 점유 셀을 추가해도 유지된다. 두 번째 경로의 모든 상태는 `B_max`와 충돌하지 않으므로 셀을 제거한 임의의 `B`에서도 충돌하지 않는다. 경로에는 first-success 회전 선택이 전혀 없으므로, 셀 제거로 앞선 킥이 성공해 선택 경로가 바뀌는 문제도 없다. 목표 상태 자체도 경로에 포함되므로 target collision 없음이 함께 확인된다.

두 사실을 결합하면 구간 내 **모든** `B`에서 해당 lock이 가능하다. 한 구간에 선택 가능한 장애물이 k개 있어도 `2^k`개의 보드를 열거하지 않는다. 이 증명이 필요한 비용은 최대 보드의 행 closure와 최소 보드의 지지 확인이다. 각 행 mask가 한 machine word에 들어가는 폭에서는 `O(h·(|S|+log w))`, 현재 10×6 구현에서는 최대 여섯 행과 고정 크기 mask 연산이다. 무제한 폭에는 추가 word 처리 비용이 생긴다. 미노 수와 무관한 한 질의의 비용이지 전체 검색의 선형 시간 보장은 아니다.

### 3.4 전체 해법·확률을 보존하는 이유

새 증명은 기존 `placement_poset`의 한 interval guard를 대체한다. 기존 컴파일러의 필요 선행관계, 정확 ILC 배치, 줄 삭제 contributor 조건, 모든 허용 frame 검사와 CountUnique 등의 admission은 바꾸지 않았다. 모든 guard가 성공한 완전한 poset certificate만 기존 piece language 및 supply coverage에 전달한다.

따라서 전체 정확성은 두 층으로 나뉜다.

- 새 정리: 반환한 양성 guard가 점유 구간의 모든 실제 보드에서 참이다.
- 기존 정리·검증: poset certificate가 성공한 후보의 허용 순서 언어와 물리적으로 가능한 순서 언어가 같고, 기존 supply product가 성공 큐의 합집합을 계산한다.

같은 종류의 구체 미노를 새 코드에서 합치지 않았고, hold automaton이나 확률 분모·가중치·coverage union도 수정하지 않았다. CountAll, trace/finesse/spin/실행 조건 등 증명의 권한을 넘는 요청은 기존 admission과 정확한 경로에 남는다. 양성 proof 하나로 모든 실행 이력의 개수나 spin label을 얻었다고 주장하지 않는다.

## 4. 반례를 규칙에 반영한 방법

| 잘못된 일반화 | 반례와 처리 |
|---|---|
| 목표 주변 두 칸만 보면 된다 | 멀리 있는 유일한 입구를 막으면 목표 근처 셀을 바꾸지 않고도 경로가 사라진다. 목표 주변 고정 반경으로 영향 범위를 자르지 않는다. |
| 미노 두 개씩 비교해서 각각 영향이 없으면 모두 무관하다 | 좌우 입구가 있고 각각 다른 미노가 막으면 하나씩 있을 때는 무회전 경로가 남지만 둘이 함께 있으면 그 경로가 사라진다. 전체 회전 경로까지 불가능하다는 뜻은 아니다. 최대 보드에서 전체 조합을 포괄하는 양성 증명을 요구한다. |
| 최대 보드에서 바닥이 있으면 된다 | 지지 미노가 빠진 최소 보드에서는 공중에 떠 있다. 접지는 최소 보드에서 증명한다. |
| 무회전 경로가 없으면 배치 불가다 | 두 칸 입구로 세운 T가 들어와 회전하면 눕힌 목표에 도달한다. 5개 연결 프로필에서 exact oracle이 이를 확인한다. 실패는 Unknown이다. |
| 최대 킥 거리 밖은 항상 무관하다 | 연속 이동과 여러 회전은 한 번의 킥 변위보다 넓은 영역에 의존할 수 있다. 이번 witness는 회전 자체를 쓰지 않아 ordered kick 간섭을 피하지만, 전체 영향 반경을 증명하지는 않는다. |
| 줄 삭제 개수만 같으면 같은 프레임이다 | 삭제한 원래 행의 위치가 다르면 좌표·지지·경로가 달라진다. 동일 `D`와 정확한 투영을 요구한다. |
| 증명을 못 만들었으니 후보를 버린다 | 증명 거절·shape/frame 범위 밖·경로 부재는 모두 기존 정확한 처리로 넘긴다. |

CEGIS식 개선 절차는 **작은 반례 → 실패 원인의 수학적 일반화 → 명시한 전제에서의 증명 → 독립 oracle과 제품 결과 비교 → 실제 감소한 연산 계수 확인** 순서로 유지한다. 전수 테스트는 구현 오류를 잡는 근거이며, 제한된 테스트 공간만으로 모든 큰 필드가 증명되었다고 보지 않는다.

## 5. 구현과 연산 재사용

- `monotone_reachability.rs`: 기존 행 closure를 재사용하고 7×4개의 실제 표준 형상에서 작은 불변 피연산자를 한 번 계산한다. 후보·보드·frame·pose 방문 상태는 이 공유 저장소에 보유하지 않는다. O 전용 exact API와 일반 양성 API는 분리했다.
- `reachability.rs`: explicit profile, exact lock mask와 frame을 전달하는 읽기 전용 진입점을 추가했다. template 구성이나 primitive 탐색을 호출하지 않는다.
- `buildup.rs`: 기존 O capability가 해당 질의에 적용되면 일반 계산을 반복하지 않는다. 그 밖의 미노는 새 증명을 시도하고, 실패하면 관측 증거 또는 기존 exact producer를 사용한다.
- `structural_order.rs`: `CLEARRA_TRANSLATION_INTERVAL`을 기존 static/O 실험과 독립된 축으로 추가했다. 기본값은 꺼짐이며 `test` 또는 `local-search-ab` 범위다. 실행 연산 계수는 후보별로 한 번 집계하고, 행마다 atomic을 추가하지 않았다.
- `placement_graph_bench.rs`: 네 번째 실험 축을 기록하고 `dependency=1, structural=1, static=0, translation=0→1` 및 `static=1` 위의 별도 비교를 지원한다. 과거 JSON에 이 축이 없으면 0으로 취급한다.

`translation_interval_attempts/guards/row_steps`는 전체 `algebraic_interval_*`의 부분집합이다. 기존 cached interval guard와도 구분한다. 실패한 후보에 쓴 계산을 숨기지 않기 위해 실패한 증명의 시도 수와 행 수도 기록한다. 이 계수는 실제 제거한 graph node·primitive state 계수와 함께 해석해야 한다.

## 6. 검증 결과

원시 로그는 `_local/artifacts/translation-interval-20261120/`에 있으며 저장소의 검토된 문서와 분리한다. 지정 Rust 1.98.1, committed lockfile과 기존 resource supervisor를 사용했다. 작은 제품 unit fixture의 worker=1은 thread-local 실험 제어를 같은 스레드에 두기 위한 기존 테스트 계약이다. 제품·벤치마크의 자동 논리 프로세서 정책을 낮추지 않았다.

### 6.1 정확성·연결 검증

| 검증 | 최종 결과 | 확인한 범위 |
|---|---:|---|
| `monotone_reachability` | 16 통과 | 7미노×4방향, 작은 보드 전수 독립 BFS, 10비트 closure, frame·지지·회전 반례, 기존 O 정확성 |
| `static_influence_integration_tests` | 8 통과 | 5개 profile의 모든 실제 identity·coverage·개별 확률, cold summary, CountAll·동일 kind·hold·줄 삭제, Unknown 비용 반례와 실제 비-O 양성 guard |
| `structural_graph_probability_tests` | 6 통과 | 기존 poset 언어·부분집합/순서 독립 oracle, support OR, 같은 kind·hold, 새 T/L clear 차단 사례 |
| 실험 feature `cargo check` | 통과 | `parallel,local-search-ab` 연결 |
| 기본 설정 `cargo check` | 통과 | 새 실험을 포함하지 않는 설정 |
| `placement_graph_bench` build | 통과 | 네 번째 축의 기록·비교 코드 |
| 같은 debug 바이너리의 one-I preflight 및 `--compare` | 통과 | 새 축 0/1 분리, 자동 CPU 한도 9, 결과 동등·차이 목록 없음 |

이는 **서로 다른 30개 지정 테스트**의 실행 결과다. 전체 저장소 CI나 배포 gate를 실행한 결과로 확대하지 않는다. 각 테스트 내부에서는 여러 profile·보드·frame을 비교했다. 예를 들어 새 구간 테스트 하나는 28개 형상×2 frame×16개 중간 보드×5 profile의 목표 lock을 독립 oracle와 대조한다. 10비트 closure는 1,024×1,024개의 free/seed 조합을 확인한다.

One-I preflight는 새 증명 대상이 아니며 translation 시도도 0이다. 이 실행은 기록·비교 배선을 확인한 smoke test이고, 단건 수 ms 시간을 알고리즘 성능 자료로 사용하지 않는다.

첫 제품 검사에서는 CoverageSummary의 표시 계약을 일반 출력과 동일하다고 가정해 실패했다. summary는 일부 숫자·identity 보고를 `not-calculated` 또는 생략으로 제공할 수 있다. 최종 검사는 summary끼리 표시 필드를 비교하고 실제 해별 coverage를 일반 출력의 구체 identity와 원래 가중치에 결합해 각 확률을 비교한다. 이후 nonzero guard를 기대했던 mixed fixture가 실제로는 비용 반례임을 확인하여, 역할을 정확히 분리하고 별도 T/L 양성 fixture를 추가했다. 실패한 기대를 양성 성능 증거로 바꾸지 않았다.

### 6.2 혼합 4미노: 정확하지만 추가 감소 없음

입력은 기존 `0xfc3f_0fc3_f0`, 높이 4, fresh bag 4회 draw, hold 없음, CountUnique다. 양쪽 모두 기존 dependency·structural·static/O 축을 켜고 **새 translation 축만** 비교했다. 840개 원래 공급 패턴과 모든 해별 확률을 동일하게 보존했다.

| Profile | 해 수 | 성공 큐/840 | 새 시도/양성 guard/행 계산 | BuildUp node 전→후 | Primitive state 전→후 |
|---|---:|---:|---:|---:|---:|
| SRS90 | 135 | 172 | 32 / 0 / 66 | 1,519→1,519 | 707→707 |
| SRS+ | 135 | 173 | 31 / 0 / 64 | 1,513→1,513 | 911→911 |
| SRS-X | 149 | 200 | 32 / 0 / 66 | 1,539→1,539 | 940→940 |
| Jstris180 | 135 | 173 | 31 / 0 / 64 | 1,513→1,513 | 910→910 |
| NoKick | 117 | 136 | 32 / 0 / 66 | 1,497→1,497 | 371→371 |

이 사례에서는 새 계산만 생겼다. 이는 증명 공식의 오류가 아니라, 현재 구성한 최대 점유 구간에서 입구가 막혀 양성 충분조건이 성립하지 않는 비용 반례다. 기본 활성화의 근거로 삼지 않는다.

### 6.3 T/L: 부분 증명은 성공하지만 필요한 선행 간선이 빠져 있음

초기 board는 `0x3f1fc3f7`, 높이 3이다. 검사한 구체 identity는 다음과 같다.

| 미노 | 원래 셀 | Mask |
|---|---|---|
| T | `(0,1), (1,1), (2,1), (1,2)` | `0x201c00` |
| L | `(3,0), (3,1), (3,2), (2,2)` | `0xc02008` |

`D=0`에서 L은 원래 0행의 유일한 contributor다. 0행이 아직 삭제되지 않았다는 조건은 L을 최대 보드에서 제외한다. 이 구간에서 T는 옆으로 진입하며 새 증명이 성공한다. 반면 L을 먼저 놓으면 원래 0행이 삭제되어 `(board,D)=(0xff7f8,1)`이 된다. T 모양의 공간은 남지만 들어오는 경로가 사라진다.

각 5개 profile의 독립 물리 oracle가 이 **구체 identity의 정확한 순서 언어가 `{TL}`**임을 확인했다. fresh bag 2회 draw·hold 없음에서는 이 identity의 성공 확률은 `1/42`다. 같은 초기 필드의 다른 ILC identity까지 포함한 제품 전체 결과는 아래와 같다.

| Profile | 전체 해 수 | 전체 성공 큐/42 | 새 시도/양성 guard/행 계산 | 새 전체 graph bypass | BuildUp node 전→후 | Primitive state 전→후 |
|---|---:|---:|---:|---:|---:|---:|
| SRS90 | 3 | 3 | 5 / 1 / 10 | 0 | 11→11 | 28→28 |
| SRS+ | 3 | 3 | 5 / 1 / 10 | 0 | 11→11 | 32→32 |
| SRS-X | 3 | 3 | 5 / 1 / 10 | 0 | 11→11 | 25→25 |
| Jstris180 | 3 | 3 | 5 / 1 / 10 | 0 | 11→11 | 32→32 |
| NoKick | 1 | 1 | 5 / 1 / 10 | 0 | 8→8 | 15→15 |

**이 예는 poset 자체의 표현 불능 사례가 아니다.** 정확한 관계는 `T≺L` 하나로 표현할 수 있다. 현재 compiler가 support/row-contributor 필요조건만으로는 이 이동·clear 기반 간선을 도출하지 못한다. `D=1`에서 translation 실패를 불도달로 바꾸지 않은 것은 올바른 동작이다. 완전한 certificate가 되지 않아 정확한 producer로 돌아갔고, 따라서 부분 양성 증명이 실제 검색 비용 감소로 이어지지 않았다.

### 6.4 범위를 제한한 비용 반례 조사

별도의 임시 Rust discovery로 높이 3·4, 폭 10 안의 왼쪽 6열, 서로 다른 종류의 2·3미노, 실제 표준 shape의 겹치지 않는 배치만 조사했다. 최소 x=0으로 정규화하고 초기 완성 행이 있는 경우를 제외했다. 최초 catalog 512회 상한에 닿은 뒤, 같은 조합 상한 2,000,000 안에서 catalog 상한만 32,000으로 늘렸다. 이는 분석 프로그램의 유한 사례 상한이며 제품 worker·메모리 정책 변경이 아니다.

- 열거한 조합: **1,831,644**.
- 겹침·종류·정규화·초기 행 조건을 만족한 후보: **65,072**.
- 초기 접지가 있고 직하강은 막히지만 비-O 옆 진입이 있는 후보: **2,257**. 실제 ILC catalog와 guard를 모두 확인했다.
- 비-O 양성 guard를 얻었으나 전체 poset가 Unknown인 후보: **255**.
- 이 조사 영역에서 새로 완성된 전체 poset: **0**.

조사 프로그램은 원하는 전체 양성 fixture를 발견하지 못했음을 명시하는 assertion으로 종료했다. 이를 통과한 회귀 테스트로 세지 않는다. 임시 코드는 원시 artifact에 보존하고 제품 테스트 모듈에서 제거했으며, 발견한 T/L 반례만 작은 고정 회귀로 남겼다. 이 결과는 **명시한 제한 영역의 관측**으로, 모든 필드에서 새 증명이 무용하거나 전체 poset 성공이 불가능하다는 정리가 아니다.

### 6.5 성능 주장과 큰 검색의 한계

이전 O 전용 roof 회귀는 여전히 graph node 21→17, primitive state 87→74를 재현했다. 이는 기존 O 규칙의 성과이고 이번 비-O 일반화의 새 효과로 합산하지 않는다.

이번 변경에 대해 큰 P7P4의 시간 개선율을 제시하지 않는다. 새 비-O 제품 사례에서는 graph/primitive 감소가 없었고, 이전 큰 실행의 종료 원인 진단도 남아 있다. 현재 작은 사례의 증명 비용이나 preflight 시간을 사용자의 12논리 프로세서 4분 기록과 비교하지 않는다. 후속 양성/부정 관계 재사용의 효과와 M01 종료 진단이 확보된 다음 같은 바이너리·입력·자동 worker 조건으로 큰 검색을 재개한다.

## 7. Rust 전환과 다음 작업의 실제 범위

이번 감사에서 Rust에는 이미 ILC 투영 필터·정확 realization·APDP parent index·geometry residual family·shared reachability template·canonical supply automaton·symbolic coverage가 존재함을 확인했다. 다음 항목은 내부 연산의 계약을 맞추는 실질적인 전환 작업이다.

- **행 사상:** Rust 10×6 adapter와 C의 더 넓은 행 범위를 구분하고, 동일 `(height,D)`의 사상 결과를 적절한 owner 안에서 재사용한다.
- **킥 정규화:** reachability·extended reachability·finesse가 같은 ordered transition 의미를 공유하게 한다. 동일 lock mask가 orientation·spin·kick index까지 같다는 가정은 금지한다.
- **정확 realization:** 약한 required-deleted-row 필터와 exact clear-state domain을 구분한 채 소비자를 연결한다.
- **ILC/APDP:** 같은 투영식·full-parent domain을 다시 구성하지 않거나 영향받은 부분만 갱신한다. 자료구조 비트 폭·캐시 라인·hash layout 조정은 이 작업의 우선순위가 아니다.
- **물리·목표 증거:** candidate 검증에서 얻은 완전한 event/evidence를 후속 scoring에서 재사용할 수 있는지 확인한다. 첫 witness만으로 B2B/모든 실행 coverage를 대체하지 않는다.
- **API cutover:** Rust 커널 미구현, 서비스 연결 누락, C도 미지원, Rust만 지원, ABI가 아닌 식별자 변환 의존성을 각각 분류한다.
- **메모리:** request 불변값, worker scratch, candidate 증거, 결과의 수명을 맞춘다. Rust 전환만으로 대형 graph 보유량이나 중복 계산이 사라진다고 가정하지 않는다.

이 작업들과 이번 CEGIS 개선은 병렬로 진행할 수 있다. 공통 선행조건은 모든 작업을 기다리는 큰 완료 장벽이 아니라 행 프레임·normalized transition·증거 권한·소유권에 대한 작은 계약이다. 같은 hot 파일의 수정과 큰 벤치마크만 순차 통합한다. 상세 88개 작업과 병렬 인계 단위는 통합 로드맵에 유지한다.

### 남은 알고리즘 과제

이번 구현은 완성된 tiling candidate의 기존 guard에 연결했다. **타일링 이전의 관계 생성·증분 업데이트, OR support를 완전히 표현하는 관계, 회전·킥을 포함한 저비용의 정확한 전체 관계, B2B event graph는 아직 완료하지 않았다.** 현재 poset가 표현하지 못하는 관계와 증명이 실패한 후보는 기존 정확한 경로에 남는다.

다음 단계는 실패한 interval을 원인별로 수집하고, 동일 projection·support·corridor를 여러 후보에서 재계산하는 부분을 일반화하는 것이다. 관계 생성 비용을 줄이기 위해 기존 generation/owner/frame 무효화 조건을 보존하는 공통 조각을 설계한다. 이후에만 geometric residual family에 실행 관계를 연결한다. `(remaining,counts)`가 같다는 이유만으로 현재 지지·입구·줄 삭제·goal 이력까지 같다고 취급하지 않는다.

### 이번 반례에서 도출한 다음 작은 증명

첫 후보는 **목표로 들어오는 모든 마지막 이동의 출발 상태가 불가능함을 보이는 부정 certificate**다. 목표가 sky seed가 아니라는 전제 아래, 좌우·하강 역상과 실제 프로필의 모든 회전·킥 역상을 포함한 상위 집합을 만든다. 이 모든 출발 상태가 경계 밖이거나 보장된 최소 점유에 충돌하면 목표에 도달하는 마지막 간선이 없으므로 불도달이다. 모든 후보를 포함한 상위 집합을 부정할 때는 ordered kick 우선순위를 생략해도 sound하다. 하나라도 남으면 Unknown이며, 일부 후보만 확인하고 부정할 수 없다.

T/L의 `D=1` 사례는 실제 5개 프로필에서 이 마지막 한 수 분석으로도 차단을 설명할 수 있다. 그러나 이번 코드에 일반 부정 compiler를 추가하지는 않았다. 회전 자체의 cycle·동일 footprint 상태, sky seed, frame 및 target mask를 포함하는 범용 계약을 먼저 확정해야 한다.

그다음 이 부정 결과를 **그 frame·점유 조건에 한정된 관계 조각**으로 저장하고, 같은 조건에서의 양성 조각과 합성한다. 특정 두 미노 사례에서 필요한 `T≺L`을 알아냈다는 이유로 더 큰 필드에서도 무조건 간선을 추가하지 않는다. 다른 미노가 추가 줄 삭제나 새로운 입구를 만들 수 있기 때문이다. 전체 후보 인증이 실패해도 유효한 부분 증명이 버려지거나 다시 계산되지 않도록 owner·규칙·구체 placement·frame·점유 조건·증거 권한을 함께 재사용하는 것이 다음 구현의 핵심이다.

## 8. 근거와 수정 위치

### 저장소 근거

- [작업 규약](../../AGENTS.md), [SRP 작업 규약 설명](../srp-debt.md), [검증 범위](../architecture-validation.md)
- [12개 연산군과 병렬 작업 로드맵](rust-core-and-relation-roadmap-2026-11-20.md)
- [행 closure와 양성 구간 증명 및 독립 oracle 테스트](../../crates/clearra-core-executor/src/backend/wasm_cpu/monotone_reachability.rs)
- [이동 계약·sky seed·연결부](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability.rs)
- [기존 poset의 필요·충분조건과 행 contributor admission](../../crates/clearra-core-executor/src/backend/wasm_cpu/placement_poset.rs)
- [BuildUp 연결](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs), [실험 제어·계수](../../crates/clearra-core-executor/src/backend/wasm_cpu/structural_order.rs)
- [제품 결과·확률·fallback 비교](../../crates/clearra-core-executor/src/backend/wasm_cpu/static_influence_integration_tests.rs)
- [T/L의 정확한 순서와 clear 차단 고정 회귀](../../crates/clearra-core-executor/src/backend/wasm_cpu/structural_graph_probability_tests.rs)
- [4축 벤치마크 기록·비교](../../crates/clearra-core-executor/examples/placement_graph_bench.rs)
- [이전 O/영향 범위 보고서와 큰 검색의 미완료 기록](static-movement-influence-2026-11-20.md)

### 외부 설계 기준

[Google Rust Style Guide](https://google.github.io/styleguide/rustguide.html)의 API 규칙 적용 범위, 불필요한 일반화 회피, 소유권·borrowing 및 필요한 trait만 도입하는 원칙을 참고한다. Chromium 전용 파일·crate 배치를 Clearra의 보편적 강제 규칙으로 적용하지 않는다. 사용자가 정한 성능·메모리 우선의 작업 규약이 이 프로젝트의 실제 결정 기준이다.

[Rust 공식 문서의 객체지향 특성 설명](https://doc.rust-lang.org/book/ch18-01-what-is-oo.html)처럼 Rust에는 데이터와 메서드·캡슐화·trait 기반 다형성이 있지만 클래스 상속을 전제로 하지 않는다. 이 계획에서 말하는 객체화는 실제 상태와 연산·수명의 소유자를 명확히 하는 것이며 클래스 계층이나 타입·파일 수를 늘리는 목표가 아니다.
