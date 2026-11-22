# 의존성 주 경로: 불가능한 킥 간선 제거와 관측 정책의 소비

이 문서는 [조합론적 보드 가족 증명](combinatorial-board-family-proof-2026-11-22.md)의
E2 정리를 실제 immutable movement template에 적용하는 작업을 기록한다.
목표는 잘못 허용하거나 제거하는 해법 없이 전체 탐색 시간을 줄이고,
동일한 정확한 고정 타일링 관계를 oracle와 see-7이 소비하도록 하는 것이다.
큰 탐색을 위한 별도의 자료구조 미세 최적화는 이번 작업에 포함하지 않는다.

## 1. 이번에 제거하는 간선

점유 가능한 보드 셀의 집합을 `U`, 실제 점유 집합을 `B ⊆ U`라 하자.
각 pose의 collision-read mask를 `M(v) = footprint(v) ∩ U`로 정의한다.
회전 전 pose의 마스크를 `S`, 순서가 있는 킥 후보의 마스크를
`T_0, ..., T_k`라 하며, 모두 같은 물리 좌표계의 비트 마스크다.
벽·바닥·천장에 대한 pose의 기하 유효성은 이 마스크와 별도로 유지한다.
점유할 수 없는 sky 셀을 앞 후보를 막는 셀로 잘못 세지 않는다.
회전 `k`가 실제로 선택되려면 다음 조건이 동시에 필요하다.

\[
B\cap(S\cup T_k)=\varnothing,
\qquad
\forall j<k:\quad B\cap T_j\ne\varnothing.
\]

기하적으로 영역 밖인 앞 후보는 원래부터 실패하므로 위의 충돌 조건에서 제외한다.
이미 유효한 후보 순서를 압축한 forward 목록과 원래 킥 인덱스의 대응은 보존한다.

### 모든 보드에 대한 제거 정리

어떤 유효한 앞 후보에 대해 `T_j ⊆ S ∪ T_k`라면 후보 `k`는 어떤 보드에서도 선택될 수 없다.
`B ∩ (S ∪ T_k) = ∅`이면 그 부분집합인 `T_j`도 점유 셀과 만나지 않아
먼저 성공하기 때문이다.
따라서 그 역방향 간선을 제거해도 모든 `B`에 대해 실제 전이 관계가 같다.

반대로 기하적으로 유효한 source와 후보 `k`에 대해,
모든 유효한 앞 후보가 `S ∪ T_k` 밖에 셀을 하나 이상 가지면,
`B = U \ (S ∪ T_k)`를 택하여 앞 후보를 동시에 모두 막을 수 있다.
여기서 `U`는 표현 가능한 점유 셀의 집합이다. 이 보드는 회전 전후 셀은 막지 않는다.
따라서 이 제거 조건은 임의 점유 보드에서 간선이 존재할 수 있는지를 정확히 판정한다.
고정 타일링에서 실제로 나오는 보드는 그중 일부이므로 이 단계는 보수적이다.
타일링의 미노 상관관계를 독립 셀로 잘못 바꾸어 추가 간선을 제거하지 않는다.

이 정리는 특정 미노, 킥 오프셋, 줄 수에 의존하지 않는다. 단,
기저 movement template의 좌표·천장·spawn 의미가 정확하다는 전제는 그대로 필요하다.
이 정리 자체가 임의 사용자 킥 테이블의 천장 정당성을 대신 증명하지는 않는다.

### 경로와 확률이 보존되는 이유

각 보드에서 실제 전이 관계를 `E_B`, 제거 후 관계를 `E'_B`라 하면
위 정리로 `E_B = E'_B`다. 동일한 초기 pose 집합에서 시작해
길이 `t` 이하 도달 집합에 대해 귀납하면 모든 `t`에서 도달 집합이 같다.
따라서 최소 고정점, full four-cell lock 판정, 고정 타일링의 완성 순서 언어가 같다.
같은 공급·hold 규칙으로 그 언어를 소비하면 각 원래 큐의 허용 여부도 같다.
성공 큐의 합계가 우연히 일치하는 것보다 강한 성질이다.

### 반드시 보존할 앞 후보 읽기

어떤 간선의 *선택 가능성*이 없더라도 그 후보의 목적 셀은 뒤 후보의
첫 성공 판정에 계속 필요할 수 있다. 따라서 다음은 서로 다른 자료다.

- 역방향 탐색의 incoming 간선 목록: 증명된 불가능 간선을 제거한다.
- forward 후보 목록과 `prior_target_count`: 원래 순서와 앞 후보를 그대로 둔다.

역방향 목록을 줄였다는 이유로 forward 후보 목록까지 압축하면 정리가 깨진다.
구현은 템플릿을 만들 때만 이 검사를 수행한다. prefix 질의마다 보드 큐브를
다시 분석하거나 추가 도달성 탐색을 하지 않는다.

## 2. oracle와 see-7의 공통 관계, 서로 다른 소비 의미

고정 타일링 관계는 미노 종류가 아니라 구체적인 operation ID의 부분집합을 소유한다.
원래 행에서 삭제된 행과 압축된 좌표도 그 관계를 만든 `CandidateGeometry`에 귀속한다.
같은 모양의 다른 후보를 나중에 바인딩하여 증명 결과를 재사용하지 않는다.

oracle는 전체 큐를 알고 완성 가능한 종류 순서가 하나라도 있는지를 묻는다.
따라서 종류 단어 언어의 정확한 합집합을 기존 공급/hold 연산기에 넘길 수 있다.

see-7은 미래 공개 전에 실제 배치를 선택해야 한다. 종류만 같은 두 배치의
자식 언어를 먼저 합친 뒤 미래를 공개하면, 선택 시점을 뒤로 미루는 잘못된
허용이 생길 수 있다. 단순 종류 언어의 결정화는 이 정책 의미를 보존하지 않는다.

### 연결 반례

처음 일곱 미노가 같고 숨겨진 다음 미노가 다른 두 큐를 생각하자.
첫 `I`의 물리 배치 `a`는 큐 1에서만 완성되고 다른 배치 `b`는 큐 2에서만 완성된다.
각 큐의 가중치가 `1/2`일 때, 공개 전 배치를 정하는 올바른 성공률은 `1/2`다.
`I` 종류만 보고 두 자식 언어를 합치면 두 큐를 모두 허용하여 `1`이 된다.
비균등 가중치 `1/4, 3/4`에서도 올바른 최댓값은 `3/4`이며 `1`이 아니다.

회귀는 P7P4와 같은 11-draw/10-operation 형식의 합성 관계로 이 선택 순서를
직접 검증한다. 이 합성 관계를 실제 4L 타일링에서 발견한 반례라고 주장하지 않는다.
실제 기하 관계의 검산은 아래 별도의 frame/realization 회귀와 전체 P7P4 비교가 맡는다.

### 정확한 물리 행동 합집합

한 행동의 키를 `(미노 종류, 현재 물리 좌표에서의 full four-cell lock mask)`로 둔다.
동일한 물리 행동을 수행한 뒤에만 여러 후보의 후속 관계를 합친다.
다른 lock mask는 종류가 같아도 서로 다른 선택으로 남긴다.
기존 관측 정책 평가기가 이 선택의 최댓값을 구한 다음 공급 공개를 진행한다.

보장 범위는 Boolean PC다. 같은 초기 보드에서 같은 물리 행동 이력은 같은 보드와
같은 line-clear 결과를 만든다. 길이에 대한 귀납으로 공통 이력의 물리 보드가 같고,
그 이력 이후 가능한 완성 언어는 정확히 자식 언어들의 합집합이다.
따라서 같은 행동에 한해 결합하는 것은 타일링 선택을 불필요하게 일찍 강제하지도,
실제 배치 선택을 미래 정보가 생길 때까지 미루지도 않는다.

이를 식으로 쓰면 공통 행동 이력 `h`와 양립하는 후보 상태의 집합 `R(h)`에 대해

\[
R(ha)=\{\operatorname{child}(s,a):s\in R(h),\ a\text{가 }s\text{에서 허용됨}\}
\]

이다. 빈 이력에서 모든 root를 넣고 이 갱신을 귀납하면 원래 후보들의 완전한
물리 행동 언어의 합집합과 정확히 같다. 합치는 root들은 동일 초기 보드·규칙·
공급 계약·catalog에 속하고 같은 operation 수를 가진다. 현재 exact-cover
후보의 operation 수는 `required_cells / 4`로 같아 이 조건을 만족한다.
후보별 원래 행 frame과 residual 상태 자체를 같다고 가정하지는 않는다.

operation ID는 relation 안에서 유지한다. 서로 다른 미노를 종류가 같다는 이유로
한 번만 사용하는 처리를 하지 않는다. line clear 이후의 행동 mask는 저장해 둔
원래 행 projection으로 구하며 이동/BuildUp 탐색을 다시 실행하지 않는다.
노드 interning은 hash가 같은지에만 의존하지 않고 전체 행동·자식 목록을 비교한다.

관측 정책은 같은 관측 상태 `z`에서 먼저 행동을 선택한 뒤 공개된 관측 `o`별
성공 질량을 합친다. 기존 공급 automaton의 hold 선택도 이 행동에 포함한다.

\[
V(z)=\max_a\sum_o V(z[a,o]).
\]

남은 lock 수에 대한 귀납으로 동일 물리 행동 언어는 이 최적 성공 질량을 보존한다.
이는 [현재 공개된 Queue Knowledge 계약](../setup-search.md#queue-knowledge)을 따른다.
`StoreCurrentUseNext`는 hold 선택과 lock을 하나의 행동으로 결정하고, lock 이후
공급 cursor에 따라 새 조각을 공개한다. 빈 hold 직후 preview를 먼저 공개하고
그다음 lock 위치를 고르는 정책은 별도의 관측 계약이며 이번 범위에 포함하지 않는다.

동률 최적 정책의 대표 성공 큐 bitset은 node ID와 tie ordering에 따라 달라질 수 있다.
최적 질량과 실행 가능한 정책은 보존하지만, 별도의 canonical action ordering 없이
기존 대표 bitset까지 같다고 주장하지 않는다. 아래 전체 P7P4의 해법별 큐 집합
동등성은 oracle 계약에 대한 검산이다.

스핀, 입력 이력, lock delay, finesse 비용 등 mask가 같아도 달라질 수 있는 의미는
이 동치의 적용 범위가 아니다. 자원 한계나 취소는 명시적인 오류/중단이며 빈 언어나
성공률 0의 완전한 결과로 바꾸지 않는다. 기존 대표 replay를 전역 see-7 선택 정책의
증인이라고 새로 표시하지도 않는다.

Trace 출력이 필요한 경우 `representative_lock`은 각 realization의 충돌·지지를
확인하고 harddrop 증명 또는 그 단일 `[lock]`의 exact target-lane 판정을 통과한
자세만 선택한다. 동일 footprint의 첫 catalog orientation을 무조건 사용하지 않는다.
선택한 rotation/x/y는 최종 `CorePathStep`까지 유지한다. 이 대표는 한 큐에 대한
유효한 lock/hold/clear 실행열이며, finesse 버튼열이나 최종 see-7 정책 자체의 증인은 아니다.

## 3. 닫아야 하는 정확성 경계

| 경계 | 필요한 보존 조건 | 검산 |
| --- | --- | --- |
| 첫 성공 킥 | 간선만 제거하고 앞 후보 읽기는 유지 | 독립 template 구조 검사와 mutation 거부 |
| 같은 종류의 미노 | 구체적인 operation ID를 유지 | 중복 종류/물리 행동 정책 회귀 |
| 같은 compact board | 삭제 행과 원래 타일링 frame을 별도로 보존 | `0x3` 보드의 다른 deleted-row fixture |
| 같은 footprint | 허용되는 모든 회전 realization의 논리합 | `0xf87f0` I의 Zero/Two 반대 판정 fixture |
| upper prune | 완성 경로에 속하지 않는 간선만 제거 | raw operation 관계의 완성 경로 closure와 비교 |
| see-7 | 물리 배치를 선택한 다음 미래 공급 공개 | 균등/비균등 가중치의 늦은 선택 반례 |
| 종류 언어 subsumption | 물리 행동을 보존하지 않은 관계의 정책 소비를 거부 | `physical_action_complete` 권한 검사 |
| 자원 한계 | 거부를 미도달/확률 0으로 해석하지 않음 | physical owner의 용량·취소 회귀 |

product 관계는 완성 언어를 보존하기 위해 dead prefix/edge를 제거할 수 있다.
따라서 제거된 edge를 모든 부분 상태에서의 물리 미도달 증명으로 읽으면 안 된다.
독립 회귀는 raw 전체 operation 관계를 먼저 구한 뒤 완성 경로 closure를 취하여
product live edge와 비교한다. 이 둘의 계약을 구분한다.

## 4. 실행 증거

간선만의 비교 소스는 `f34f72b47f3515236babce3ee03bf3dc587a1593`이며,
제품 소비자 변경과 분리된 고정 worktree에서 release binary를 한 번 빌드했다.
네이티브 local-search-ab에서만 `CLEARRA_STRICT_GLOBAL_KICK_PRUNE=0|1`을 허용한다.
일반 제품 빌드는 이 우회 환경 변수를 읽지 않는다.

최종 제품 연결 및 반복 시간 측정은 하나의 다른 고정 release binary를 사용한다.

| 소스 식별자 | 값 |
| --- | --- |
| 로컬 빌드 커밋 | `a137c0fb8d0006bb5306a6c9831787228b7955a5` |
| 게시한 동일 소스의 커밋 | `99d33dae491a54757aaea479f449f6b4790e2582` |
| 양쪽 소스 tree | `50fca9e85ba742c3c9af9f520e1dad33f6c9125f` |
| release binary SHA-256 | `777ceebd5e02da019db42039fc4974701ffde977255e71492bd7949a9db7eec0` |

GitData 게시 과정의 부모·메타데이터 때문에 커밋 ID는 다르지만 tree가 같다.
바이너리가 보고한 `compiled_source_commit`은 로컬 커밋 `a137c0f`다.
후속 문서 커밋을 빌드한 것처럼 표시하지 않는다.

### 5L/6L과 작은 공급의 검산

현재 완료된 독립 5L/6L target 검산:

| 항목 | 결과 |
| --- | ---: |
| 높이 / 프로필 / 미노 종류 | 5·6L / 5 / 7 |
| 독립 lock 판정 비교 | 3,660 |
| 양성 / 음성 | 759 / 2,901 |
| 불일치 | 0 |
| 검사한 템플릿들의 제거 간선 합 | 85,657 |
| 순서 킥 조건 평가: 제거 전 → 후 | 400,058 → 310,973 |

이 표는 범용 정리의 제한된 실행 검산이며 전체 5L/6L 타일링의 완전열거가 아니다.
P7 preflight는 246개 해법, 5,040개 원래 큐, 79,288개 해법–큐 쌍의
정확한 coverage 파일을 BuildUp 기준과 비교하여 동일함을 확인했다.
수 밀리초 규모 P7 시간은 전체 탐색의 성능 근거로 사용하지 않는다.

추가 보드 가족 회귀는 H1–6 × 5프로필 × 7종류의 210 case,
45,360개 보드 및 1,111,200개의 full-target 판정을 독립 기준과 비교하여 불일치 0이었다.
세 셀 큐브의 E2 판정은 110,592개를 전수 비교했다.
내장 킥의 천장 검산과 사용자 상승 킥의 음성 반례도 유지했다.

### 전체 P7P4 oracle의 정확 집합 비교

입력은 빈 폭 10·4L, Jstris180, 10 lock, 11 draw, 빈 hold 사용이다.
원래 큐 우주는 `7! × 7P4 = 4,233,600`개다. 다음 네 경로를 비교했다.

| 경로 | 정규화 해법 | 해법–원래 큐 쌍 | 전체 큐 성공률 | 비교 |
| --- | ---: | ---: | ---: | --- |
| 기존 concrete BuildUp | 456,459 | 15,800,311,980 | 1 | 기준 |
| 의존성, 전역 E2 제거 전 | 456,459 | 15,800,311,980 | 1 | 정확 집합 일치 |
| 의존성, 전역 E2 제거 후 | 456,459 | 15,800,311,980 | 1 | 정확 집합 일치 |
| 토글 없는 자동 제품 경로 | 456,459 | 15,800,311,980 | 1 | 정확 집합 일치 |

각 capture는 1,857,438,345바이트이며 SHA-256은 모두
`f8b79ba66ed482548f3aa8f679cb8108b32274d16cff04b407d32543150170ea`다.
이는 hash만 비교한 결과가 아니다. `--compare-coverage`가 정규화된 해법 key별
원래 큐 집합을 직접 읽고 모든 행의 동등성을 검사했다. 후보 수 또한
29,856,840으로 같다. 이 표의 해법–큐 쌍은 원래 큐별 Boolean 존재 여부이며
모든 배치 경로의 중복 개수를 뜻하지 않는다.

자동 제품 검산의 admission은 `eligible=true`, `legacy_flag=false`,
`strict_experiment_applied=false`였다. 실험 wrapper를 거치지 않고 일반 제품
dispatch로 실행했으며, BuildUp 노드·기존 reachability state가 모두 0이었다.
기존 concrete 기준은 BuildUp 노드 637,208,218개였다.
여기서 기존 reachability state 계수가 0인 것은 의존성 증명 내부의 exact reverse
관계 계산까지 없다는 뜻이 아니다. 그 계산량은 바로 다음 표에 따로 제시한다.

### 간선 제거로 감소한 연산

다음은 동일한 간선 실험 바이너리의 제거 전후 값이다.

| 계수 | 제거 전 | 제거 후 | 의미 |
| --- | ---: | ---: | --- |
| ordered kick terms | 2,879,722,152 | 1,982,483,329 | 31.16% 감소 |
| pose domains | 654,620,603 | 490,763,066 | 25.03% 감소 |
| reverse relations | 8,528,265 | 8,528,265 | 관계 질의 수는 같음 |
| unresolved lanes | 124,505,112 | 124,505,112 | unresolved 질문 수는 같음 |
| worklist pops | 265,071,852 | 265,071,852 | 고정점 진행은 같음 |
| pose work | 1,517,673,697 | 1,517,673,697 | 같은 상태 집합을 처리 |
| 한 관계의 전역 제거 간선 최대 | 0 | 1,359 | 템플릿 생성 때 제거 |

이번 제거는 movement kick graph의 불가능 간선을 줄인다. 타일링마다 필요한
관계 질의나 모든 prefix 상태가 사라졌다는 주장은 하지 않는다. 남는 큰 구조적
성능 작업은 이 결과를 기준으로 별도 진행한다.

### 결과 저장을 제외한 반복 시간

시간 측정은 전체 논리 CPU 9개를 자동 선택하고 동일 최종 바이너리로 수행했다.
순서는 `C1 A1 B1 P1 P2 B2 A2 C2`였다. C는 concrete BuildUp, A는 의존성/E2 off,
B는 의존성/E2 on, P는 실험 wrapper 없는 자동 제품 경로다.
모든 arm은 같은 CoverageSummary 계약과 discard capture sink를 사용한다.
시간 측정과 별도로 위의 전체 coverage 비교를 먼저 완료했다.

| 경로 | 첫 실행 | 왕복 실행 | 평균 |
| --- | ---: | ---: | ---: |
| C: 기존 concrete BuildUp | 95.112초 | 97.977초 | 96.544초 |
| A: 의존성, 전역 E2 제거 전 | 76.680초 | 74.321초 | 75.501초 |
| B: 의존성, 전역 E2 제거 후 | 71.495초 | 70.981초 | 71.238초 |
| P: 자동 제품 경로 | 68.475초 | 70.078초 | 69.277초 |

각 arm이 2개 값이므로 여기의 평균은 두 값의 중앙값과 같다. 주 비교인 A→B는
**5.65% 감소**, C→P는 **28.24% 감소**다. B→P의 2.75% 차이는 실험 wrapper와
보고 계수 유무도 다른 제품 전환 비교이며, 이를 전부 간선 제거의 효과로 더하지 않는다.
모든 실행이 `valid=true`, 전체 완료, 9개 worker 및 동일 전역 coverage를 보고했고,
비교기의 `equivalent=true`, `differences=[]`를 확인했다. 정확한 해법별 큐 비교는
앞 절의 별도 capture 비교를 근거로 한다.

계수 감소가 시간 감소와 같은 비율일 필요는 없다. 이번에는 ordered kick terms를
31.16% 줄여 전체 탐색 시간을 5.65% 줄였다. 같은 상태를 다루는 다른 계산은 남아 있다.
측정은 이 환경의 각 2회 왕복 실행이며 p95나 다른 기기의 보장값이 아니다.
사용자의 12개 논리 프로세서 환경에서 관측한 약 4분과 하드웨어·소스·출력 계약이
다르므로 그 숫자를 직접 분모로 삼아 개선율을 계산하지 않는다.

대용량 capture 저장이 포함된 앞선 실행은 C 95.937초, A 74.696초, B 78.670초였다.
이 세 값은 각 1.857GB의 capture 기록을 포함한 `timing_ms.search`이며,
뒤이은 약 40–44초의 evidence hashing은 별도 시간이다. 그 실행에서는 B가 A보다
약 5.3% 느렸다. 이를 숨기거나 계수 감소만으로 시간 개선을 결론 내리지 않고,
capture를 버리는 같은 출력 계약으로 위의 별도 반복 시간을 측정했다.
위 표의 시간도 benchmark의 `timing_ms.search`다.

### 소비자 및 제품 표면 검산

- 최종 코어 전체 회귀: **1,028 통과, 0 실패, 13 명시적 ignore**.
- benchmark harness 회귀: **26 통과**. 자동 제품 admission 및 coverage 비교 오류 경로 포함.
- problem admission 회귀: **2 통과**.
- GUI/Discord 관련 선택 회귀: **93 통과**. TypeScript 계약 검사와 `ctk3` 빌드 통과.
- CLI 검사, `wasm32-unknown-unknown`의 clearra-wasm 검사, 코어 `--no-default-features` 검사 통과.
- `--no-default-features` 구성에는 현재 읽는 소비자가 없는 diagnostic count 필드의
  dead-code 경고 1개가 있다. 정상 제품/최종 benchmark 빌드 오류는 없다.
- see-7은 실제 P7 전체와 실제 10-operation identity의 24개 가중 큐에서
  hold on/off 및 concrete physical-action 기준을 검산했다. kind-only 늦은 선택
  반례와 자원 한계 오류도 회귀에 포함한다. 전체 P7P4 see-7의 완주·속도는 별도 항목이다.

### 전체 P7P4 see-7 제품 실행: 관계 한도로 미완료

추가로 동일 Rust 소스의 일반 제품 CLI를 release로 빌드하여 아래 요청을 실행했다.
`local-search-ab`를 켜지 않고, 과거 의존성 opt-in 인자도 주지 않았다.
`--queue` 생략은 fresh standard bag이며 빈 4L+hold 요청의 공급은 P7P4 전체 우주다.

```sh
clearra pc --lines 4 --hold --objective unique \
  --rule jstris-180 --queue-knowledge visible-7 --backend cpu \
  --use-all-cpu-threads --no-backend-fallback --no-tablebase --no-legal-board \
  --no-conditioned-reachability --format json
```

**이 실행은 완주하지 못했다.** 감독자의 관측 시간 28.392초에 종료 코드 1을 반환했다.

| 관측 항목 | 결과 |
| --- | --- |
| CLI 결과 종류 | `execution-failed` |
| 표면 오류 코드 | `E_PC_SEARCH_INTERNAL` |
| 내부 원인 | `wasm_observation_physical_relation_limit` |
| `result_completeness` | `incomplete` |
| `probability_complete` | `false` |
| 감독자 샘플 RSS 최대 | 566,501,376바이트, 약 540MiB |
| 감독자 메모리 pressure / timeout | 0회 / 발생하지 않음 |

따라서 이는 OS 메모리 부족이나 timeout이 아니라 물리 행동 관계 owner의
명시적 상한 거부다. 현재 관계 상한은 nodes 1,000,000 / edges 8,000,000이며
오류 코드는 둘을 구분하지 않으므로 어느 카운터가 먼저 닿았는지 이 결과만으로
단정하지 않는다. work 64,000,000과 union members 8,000,000도 별도 상한이다.
성공률 0이나 완료된 coverage로 바뀌지 않았다는 오류 경계는 실제 제품에서도 확인했다.

일반 CLI는 내부 출력이 `Trace`이며 benchmark의 `CoverageSummary`와 다르다.
현재 `requires_observation_policy()` 요청은 전역 정책 owner 때문에 분산 실행을
선택하지 않는다. 전체 CPU를 요청한 그대로 실행했지만 oracle의 9-worker 시간과
같은 병렬 폭의 성능 수치로 비교할 수 없다. 위 28.392초는 실패까지의 시간이며
완성 탐색의 성능 수치가 아니다.

진단 CLI의 SHA-256은
`7f5c60245562580de49f1332a4d9254e8e927055b985a8594751c0b5d2a58e18`이다.
CLI의 `runtime_identity`는 일반 로컬 빌드의 `unverified-local-build`였으며,
이를 배포 또는 canonical acceptance 증거로 사용하지 않는다.
감독자 run ID는 `1795310000189778630-553`이다.

**결론적으로 oracle P7P4의 정확 집합 보존과 시간 감소는 확인했지만, 대규모 see-7까지
포함한 제품 전환 완료 조건은 충족하지 못했다.** 다음 구조적 성능 작업은 동일 물리
행동 이력이라는 동치 조건을 보존하면서 전역 관계의 생성·보유량을 줄이고 이 전체
입력을 완주시키는 것이다. 과거 kind-only 합집합은 늦은 선택 반례가 있으므로
이 입력의 정확한 정책 기준으로 사용할 수 없다.

코어 검사는 `--test-threads=1`로 전역 자원 admission 충돌을 분리했다.
이는 libtest의 동시 테스트 수이며 solver의 논리 CPU 사용 수는 줄이지 않았다.
초기 실패를 정답 불일치로 숨기지 않고 다음 원인으로 분리했다.

- 서로 독립적인 테스트가 공유 자원 reservation을 동시에 요구한 충돌.
- deferred geometry 테스트가 기존 두 universe 소유량을 cap에서 빠뜨린 기대값:
  정확 cap 성공과 1바이트 부족 거부를 모두 유지하도록 테스트만 수정.
- frontier certificate checker가 E2로 불가능함이 증명된 incoming 간선까지 요구한
  기대값: 제품 helper를 호출하지 않고 원래 forward footprint로 E2를 독립 검사한다.
- 자동 제품 dispatch가 test/local operation-mask reference까지 먼저 선택한 문제:
  명시적인 실험 scope만 별도로 보존하고 일반 제품에는 해당 우회를 넣지 않는다.

주요 재현 명령은 다음과 같다. 모든 큰 탐색은 `clearra-manage runtime run`의
`benchmark-search` 프로필과 유한 timeout으로 감싸고 worker를 자동 탐지했다.

```sh
cargo test --locked -p clearra-core-executor \
  --features parallel,local-search-ab,placement-dependency-pc --lib -- --test-threads=1
cargo test --locked -p clearra-core-executor \
  --features parallel,local-search-ab,placement-dependency-pc --example placement_graph_bench
cargo check --locked -p clearra-wasm --target wasm32-unknown-unknown
cargo check --locked -p clearra-cli
```

## 5. 적용과 남는 범위

자동 주 경로의 자격은 명시적인 API 경계다. 현재 제품 어댑터의 증명된 계약은
표준 폭 10, compact 1–4L, 1–10개 고정 타일링 operation, fresh standard bag의
완전 공급 우주, SrsPlus/Jstris180 및 Boolean CountUnique PC다.
see-7도 동일 관계를 소비하되 기존 관측 공급 계약과 자원 제한을 지킨다.

이 범위에서는 false인 과거 사전 분석 옵션이 BuildUp을 선택하는 수단이 되면 안 된다.
GUI/명령 preview에서는 선택 토글을 제거한다. 과거 저장 요청의 호환 필드와
명시 요청의 unsupported 오류 계약은 계산 경로 선택과 분리한다.
CLI는 기존 alias의 파싱 호환성을 남기지만 도움말에서 opt-in으로 안내하지 않는다.
GUI와 Discord의 새 요청은 이 선택을 요구하지 않는다. CPU/Auto 요청이 적용 대상이며
명시 GPU 및 지원 범위 밖의 계약을 이 의존성 경로로 몰래 바꾸지 않는다.

CountAll, 점수/스핀, 사용자 킥, 다른 공급 계약, 더 큰 높이/operation 수는
별도 제품 계약이다. 이 작업의 P7P4 결과만으로 해당 경로까지 삭제하지 않는다.
일반 높이에 대한 수학 정리와 현재 compact 제품 어댑터의 지원 범위도 구분한다.
따라서 여기서 완료하는 것은 **자격을 만족하는 PC 요청의 제품 검증 경로를
의존성 관계로 전환하는 것**이다. BuildUp의 모든 소스 및 모든 제품의 사용을
일괄 삭제하는 작업은 아니다.

physical-action owner는 node·edge·union·work에 유한 자원 한계를 둔다.
한계에 닿으면 실행 오류를 반환하며 완료된 낮은 확률로 대체하지 않는다.
이 정확성 경계가 큰 see-7 탐색의 완주나 메모리 효율까지 보장하지는 않는다.

## 6. 작업 상태

- [x] 수정 파일: `reachability.rs`, 구현 내용: 모든 보드에서 불가능한 역방향 킥 간선을 템플릿 생성 때 제거하고 앞 후보 순서를 보존한다.
- [x] 수정 파일: `reachability_symbolic_target.rs`, 구현 내용: 5L/6L 독립 기준과 간선 제거 전후 판정을 비교하고 실제 조건 평가 감소를 기록한다.
- [x] 수정 파일: `placement_graph_bench.rs`, 구현 내용: 전체 P7P4의 해법별 원래 큐 집합 동등성과 반복 실행 시간을 확정한다.
- [x] 수정 파일: `queue_observation_physical_actions.rs`, 구현 내용: 동일 물리 행동에 한해서만 후보의 완성 언어를 합치고 공개 전 행동 선택을 검증한다.
- [x] 수정 파일: `symbolic_prefix_orders.rs`, 구현 내용: 이미 증명한 관계와 원래 행 projection을 재사용하여 oracle/see-7 소비자를 연결한다.
- [x] 수정 파일: `search_problem.rs`, 구현 내용: 검증된 PC 범위에서 과거 opt-in 값과 무관하게 의존성 주 경로를 선택한다.
- [x] 수정 파일: `SearchControls.svelte`, 구현 내용: 의존성 사전 분석 토글을 제거하고 기본 경로를 제품 요청에 반영한다.
- [x] 일반 제품 CLI에서 전체 P7P4 see-7을 실행하고 관계 상한의 미완료 결과를 기록한다.
- [ ] 전체 P7P4 see-7이 관계 한도 안에서 완주하도록 만드는 후속 구조적 작업과 재검산.
- [ ] 정확한 게시 SHA의 필요한 원격 CI 확인과 제품 승격 판단.

## 7. 게시와 제품 승격

작업 브랜치는 `codex/dependency-primary-path-20261122`다. PR을 만들지 않고
동일 tree를 GitData로 게시했다. 위 코드 커밋 `99d33dae`의 Actions 실행 조회는
0건이었다. `Candidate Preflight`는 `workflow_dispatch`로 시작하는 workflow이며,
현재 확인한 GitHub 도구에는 신규 dispatch가 없다. 로컬 검사 통과를 원격 CI 통과로
표시하지 않는다.

`AGENTS.md`의 승격 규약은 필요한 CI를 확인한 정확한 SHA의 fast-forward다.
이번 작업은 `main`이나 배포 트래픽을 변경하지 않았다. 큰 see-7에서 확인한 관계
상한과 아직 없는 원격 CI 증거를 해결하기 전에는 전체 제품 전환 완료로 판정하지 않는다.
