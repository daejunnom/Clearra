# 보드 생성 이후의 도달성·후보 집합 비용

## 최종 판정

보드 생성 이후의 주요 비용은 반복되는 target reachability와 그 뒤의 queue/Hold
coverage product였다. 가족 단위의 추가 음성 조건, 실제 source에서 도출한 공급
상한, fused buffer의 중복 쓰기 제거와 후반 계측 도구를 반영한다.

비트 집합 도달성 구현은 기능 검증을 통과했지만 **기본 dispatch는 켜지 않는다**.
P1P7P7의 밀도 후보는 원인 미상 SIGKILL로 정상 표본을 얻지 못했고, 대안 OP7P7의
정상 300초 비교에서는 후보 배출이 5.62% 적고 RSS 감소는 0.27%에 그쳤다.
준비 시간부터 환경 변동이 있어 순수 kernel 회귀율은 확정하지 않지만, 기본
승격의 근거도 없다. 최종 code commit `f557ef62f75df27026b27fb41c93134beee46ed2`의
`FULL_FORWARD_DEFAULT`와 `HYBRID_REVERSE_DEFAULT`는 모두 false다.

실제 Full6를 실용적인 완료 시간으로 줄였다는 결론은 아직 없다. 아래 수치와
수학적 구현·안전한 필요조건을 그 결론으로 확대하지 않는다. CI는 별도 사용자
승인 전까지 중단 상태다.

## 작업 범위와 실행 계약

이번 작업은 이전 총 240초 진단에서 생성 이후에 남았던 비용을 분리한다.
CI는 사용자의 별도 승인 전까지 실행·재실행하지 않는다. 전체 Full6 후보 영역을
소진하거나, 이미 주어진 100% 확률을 다시 탐색·검색·증명하는 작업이 아니다.
1미노 등 극소 입력의 성능 벤치마크도 실행하지 않는다. 아래 수학은 구현의
동등성과 안전한 필요조건에 관한 것이다.

실행 입력은 empty 10×6, SRS+, P1P7P7의 원본 177,811,200개 큐, 빈 Hold,
15 locks, FullQueueOracle, CountUnique이다. 제품 전용 확률 증거 보관 정책은
Disabled, 해별 solution probability는 Omit이다. 이 두 옵션이 일반 PC 결과의
global coverage 반환까지 생략하는 것은 아니다. 생성 cap은 unlimited인 0을 유지하고,
Auto 정책은 허용된 논리 CPU 9개를 모두 사용한다. 준비된 워커가 즉시 작업하는
기존 정책과 공유 immutable family의 소유 관계를 유지한다.

새 `full6l_bench` 옵션:

```text
run --objective unique --profile --post-prepare-seconds 300
run --pattern OP7P7 --objective unique --profile --post-prepare-seconds 300
```

`--post-prepare-seconds`는 `geometry-candidate-family` 이벤트에서 시작한다.
이 경계는 family 생성과 construction index 해제, path-count 및 row-order
테이블 준비 이후이며 task split 이전이다. 기존 전체 deadline 옵션과는
상호 배타적이다. 1초 간격 RSS 표본, 경계 profile, 취소 요청 시각과 모든
worker가 돌아온 시각을 따로 기록한다. 경계를 가로지르는
`parallel_compile_and_split` span은 후반 차분에서 제외한다.

실행은 Rust `clearra-manage`의 `benchmark-search` profile과 별도의 900초
supervisor 한계로 보호한다. 진단 창 종료는 협력 취소이며, 결과를 complete로
바꾸거나 전체 영역의 성능·확률 결과로 취급하지 않는다.

## 생성 이후 기준 계측

기준 source commit은 `212c5ccfe308945ad1f134d91b1bde492849abf1`이다.
계측 전 제품 코드는 `1ad794056bec85454c511d21394d8359184743ee`와 같다.
새 counter는 기존 후보 샘플 지점에서 worker 누적 counter의 차분만 읽는다.
이동마다 타이머나 allocation을 넣지 않았으며 제품의 non-profiling 빌드에서는
추가 경로가 컴파일되지 않는다.

기준 binary SHA-256:
`8857c6d9f823505534a1644f9f2225eeec6580f6ccc72d6cbed9c122b0e814f5`.
빌드 feature는 기본 feature를 유지한 `parallel,wasm-stage-profiling`이며
`local-search-ab`는 사용하지 않았다. 후자는 다른 prototype gate도 바꾸므로
이번 비교를 위해 켜지 않는다.

| 관측 항목 | 기준값 |
|---|---:|
| family 준비 경계 | 111.127824157 s |
| 후반 취소 요청까지 | 300.000187793 s |
| 후반 joined return까지 | 300.802953793 s |
| 취소 후 정리 | 0.802766 s |
| 전체 실행 | 411.930930231 s |
| 원본 geometry family paths | 247,794,216,021,372 |
| family nodes | 52,325,237 |
| 후반 emitted candidates, 정확한 owner 합계 | 31,502,265 |
| row-order로 제외한 subtree 수, 정확한 owner 합계 | 143,517,300 |
| source-prefix checks, 정확한 owner 합계 | 171,153,515 |
| source-prefix로 제외한 subtree 수, 정확한 owner 합계 | 15,054,771 |
| 후반 1초 표본 RSS peak | 6,121,734,144 B |
| supervisor process-group 표본 peak | 6,121,332,736 B |
| Linux process VmHWM | 6,121,111,552 B |
| 완료 상태 | `complete=false`, `Cancelled` |

서로 다른 시점·관측 방법의 RSS peak를 같은 counter로 취급하지 않는다.
supervisor receipt `1795385221713889093-7-runtime`에는 timeout 종료,
memory pressure event, automatic retry가 없다. child exit 1은 예상한
협력 취소이며, supervisor가 강제 종료한 것이 아니다.

| 후반 stage | caller와 worker 누적 시간 | work |
|---|---:|---:|
| BuildOrder / reachability | 1,099.156113155 s | 901,753,501 graph work |
| queue / Hold language product | 208.666824684 s | 613,134,100 product work |
| geometry advance | 86.773404666 s | 샘플 가중 invocation 기준 |
| source-prefix | 52.967439477 s | checks는 위의 정확한 owner 합계 |
| candidate projection | 5.422757965 s | slot-space 단위이며 실제 방문 수가 아님 |

시간은 중첩·겹침이 있는 worker 누적 샘플값이다. 서로 더하여 wall time 비율을
만들지 않는다. raw family path 수는 물리적으로 유효한 해의 수가 아니며,
subtree 제거가 있으므로 현재 candidate 배출 속도로 전체 완료 시간을 외삽하지 않는다.

### 실제 도달성 경로

다음 값은 후보 샘플에서 기존 counter 차분을 가중한 **추정치**다.

| counter | 추정 work |
|---|---:|
| lock queries | 745,393,757 |
| cache reachable hits | 3,318,477 |
| cache unreachable hits | 6,108,196 |
| cache key misses | 735,760,213 |
| partial searches | 735,967,084 |
| exhaustive searches | 0 in samples |
| generated states | 13,547,401,578 |
| harddrop queries | 2,422,477,202 |
| harddrop hits | 1,471,536,620 |

lock-query 대비 cache hit 추정 비중은 약 1.26%이고, partial search당 generated
states는 약 18.4이다. 하드드롭과 lock query는 서로 다른 호출 표면도 포함하므로
두 counter를 같은 분모로 섞지 않는다. 일반 API의 partial은 짧은 forward desired
search도 포함할 수 있다. 이 Full6 concrete BuildOrder 경로는 target reverse를
사용하지만, counter 이름 자체를 reverse/forward state count로 바꾸지 않는다.

이 관측 때문에 full forward traversal만 치환하는 안은 주 개선 대상으로
삼지 않는다. 길어진 target reverse를 집합 계산으로 넘기는 안과, 완성된 후보를
만들기 전에 여러 후보를 제외하는 필요조건을 검토한다.

## 회전별 anchor 비트 집합

보드 점유 집합을 B, 회전 r의 정규화된 네 셀 offset을 S_r, 벽·바닥·기존 sky
범위에서 유효한 anchor 집합을 V_r라고 하자. 비충돌 anchor 전체는 다음과 같다.

\[
F_r(B)=V_r\setminus\bigcup_{c\in S_r}(B-c).
\]

구현은 registry의 정규화된 실제 shape를 사용한다. 회전축에 따른 shape-origin
보정과 raw kick을 합산하는 기존 `normalized_kick_delta_wide`를 재사용한다.
별도의 회전축이나 nominal profile ID로 imported table을 재구성하지 않는다.
행 경계를 가로지르는 비트 shift는 source-column mask로 막는다.

회전 r에서 s로 가는 ordered kick의 변위를 δ_j라 하면, j번째 kick이 실제로
선택되는 source는 다음 집합이다.

\[
G_{r,j}=F_r\cap\tau_{-\delta_j}(F_s)
\cap\bigcap_{i<j}\overline{\tau_{-\delta_i}(F_s)}.
\]

앞선 destination이 **free인지**가 뒤 kick을 막는다. 앞선 destination이 아직
방문되지 않았거나, 이미 방문되어 큐에 다시 들어가지 않는다는 이유로 뒤 kick을
허용하지 않는다. 이 차이를 생략하면 SRS의 ordered first-success 의미가 바뀐다.

forward 도달 집합은 기존 실제 sky seed에서 이동과 회전의 최소 고정점이다.
reverse는 target에서 시작하여 아래 preimage를 누적한다.

\[
R_r\gets R_r\cup\left(G_{r,j}\cap\tau_{-\delta_j}(R_s)\right).
\]

이동도 기존 down/left/right edge의 역상으로 계산한다. reverse에서 upward를
방문하는 것은 실제 downward 이동의 predecessor이며 새 조작을 추가하는 것이 아니다.
기존 sky seed와 교집합을 만나야 양성으로 끝나고, frontier를 소진해야 음성이다.

초안 hybrid는 scalar가 32 pose를 처리했는데 frontier가 남은 경우에만 전환한다.
이미 발견한 scalar queue 전체는 reached, 미처리 suffix만 pending에 넣는다.
기존 32 pose를 다시 전개하지 않는다. 짧은 query는 plane과 kick adapter를 만들지 않는다.
새 scratch는 2 / 8 / 64 word specialization의 고정 stack 공간이며, query별 heap이나
worker별 retained table을 추가하지 않는다. 현재 adapter 범위는 width 1–10,
height 1–6, board area ≤64, 기존 sky를 포함한 회전별 anchor plane ≤64개의
u64(최대 4,096 anchors)이다. 실제 ordered kick은 transition당 최대 32개,
전체 최대 256개일 때 적용한다. 이를 넘으면 sky나 kick 목록을 자르지 않고
scalar로 진행한다. trace 소비자도 scalar를 유지하며 scoring, finesse,
qualified proven-exit composition의 trace를 Boolean 집합 답으로 대체하지 않는다.

## 후보 가족의 첫 배치 필요조건

기존 family는 Append/Union/Product로 이루어진 row-set 식이다. 모든 leaf를
꺼내기 전에 '이 가족 중 첫 lock으로 사용할 placement가 하나라도 있는가'를
요약할 수 있다. 초기 보드가 비면 frame zero에서 실제로 인스턴스화되고 바닥에
접지한 pose는 hard drop 경로가 비어 있다. 이 조건이 어느 placement에도 없으면
가족 전체가 불가능하다. 초기 보드가 비어 있지 않으면 이번 요약은 Unknown이다.

기존 6L 내부 원본 행 1..=4의 24개 순서 요약을 담는 u32에 남는 한 비트를 사용한다.

| 결합 | 기존 행 순서 24 bits | 첫 lock 가능성 bit |
|---|---|---|
| Append / Product / 선택 prefix / continuation | AND | OR |
| Union | OR | OR |

첫 lock bit는 기존 하위 bit를 덮거나 양성 buildability를 부여하지 않는다.
별도 family 배열, 노드 크기 증가, path count 변경 없이 필요조건을 강화한다.

## 실제 큐에서 도출하는 공급/Hold 상한

15개 원본 미노를 모두 소비하고 초기 Hold가 비어 있는 범위에서, 첫 n=7 locks는
Hold가 있으면 처음 m=8 draws, Hold가 없으면 m=7 draws 안에서 공급되어야 한다.
실제 source의 prefix 최대 종류 수를 P, suffix 최소 종류 수를 L, 실제 가능한
전체 target inventory를 T라고 하면 각 종류의 안전한 상한은 다음과 같다.

\[
C_k=\min(P_k,T_k-L_k).
\]

factorized draw-without-replacement atom의 실제 choices와 draw 수로 P와 L을
요청 시 한 번 계산한다. 고정 큐는 실제 15개 원본 하나를 읽는다. 원본 큐 전체를
전개하지 않는다. P1P7P7의 middle cut에서는 T=2+e_p, L=1이므로 C=1+e_p가
도출된다. 이 수학 조건과 실제 target domain이 맞으면 기존 O(7) specialization을
재사용하며, 매 prefix마다 7개 target DP를 새로 돌리지 않는다.

선택된 기하 prefix에서 삭제 행 상태 D에 아직 positive realization이 준비되지
않은 종류별 연산 수를 U(D)라 하면, 미래 placement를 전부 즉시 사용 가능하다고
완화해도 준비된 수는 A(D)=T-U(D)를 넘지 못한다. 따라서

\[
\sum_k\min(A_k(D),C_k)\ge n
\]

은 첫 n개 배치의 필요조건이다. 실패하면 실제 선택된 row contributors의 합집합만
필수 연산으로 취급하는 기존 64 row-state 완화를 계속한다. 필수 연산 총수 n과
종류별 C를 넘는 전이는 제외한다. 후보 prefix에 아직 여러 T가 가능하면 모두
고려하며, 하나라도 Unknown이면 가족을 유지한다. 현재 자동 적용은 native 64-bit,
빈 초기 Hold와 cursor 0, 전체 15 draws/15 locks, 완전한 factorized backing 또는
단일 실제 고정 큐 범위다. 실제 target이 8개를 넘거나 지원하지 않는 backing이면
선택적 source cut만 생략한다. 물리 검증 및 원본 ordinal/queue/Hold product의
양성 판정 권한은 유지한다. 따라서 특정 문자열 대신 실제 공급 계약에서 도출하는
최적화이지만, 모든 backing 표현을 새 source cut이 지원한다는 뜻은 아니다.

## 메모리와 다음 경계

기준 실행 마지막 9개 worker checkpoint의 retained-capacity owner counter에서
StandardBag private owner는 451,403,104–471,752,704 B(약 451.4–471.8 MB),
reachability owner는 1,760,832–3,432,000 B(약 1.76–3.43 MB)였다. 이 owner counter는
process RSS와 다르다. hit가 낮다는 이유로 exact-board cache를 확대하거나,
모든 family node에 row-state×kind 표를 보관하지 않는다.
52,325,237 nodes에 64×7개의 u8을 추가하면 약 23.4 GB가 필요하다.

fused canonicalization은 역순 순회에서 모든 node를 기록하므로 선행 전체
EMPTY fill을 생략하고 dead node도 직접 EMPTY로 덮는다. 충분한 길이로 할당된
graph_classes 버퍼를 재사용할 때 N을 현재 node 수, L을 live node 수라 하면
기존 N+L writes를 N으로 줄인다. 최초 확장 시 필요한 resize 초기화는 남으며,
새 heap을 추가하거나 non-fused 초기화 의미를 바꾸지 않는다.

더 깊은 가족 검증에는 D_r(F)={S−{r}: r∈S∈F}와 실제 board, original deleted-row
frame을 묶어야 한다. 한 action이 가능한 것만으로 모든 후보를 accept하거나
CountUnique를 합산할 수 없다. 현재 BuildOrder에는 source-prefix가 edge를 줄이는
경로도 있으므로 기존 suffix language를 source/Hold 독립으로 공유할 수 없다.
큰 derivative arena나 전체 suffix cache는 이번 변경에 추가하지 않는다.

## 로컬 기능 검증과 빌드

후보 `5bae13f10dd2cefdbf00af2a897f8ad4b5298b5f`는 기본 reverse hybrid를 켜고,
full forward plane 치환은 끈 상태로 기능 검증했다. 단일 실제 큐의 deferred
target preparation을 읽는 수정도 포함한다. 최종 결과는 **65 passed, 0 failed,
2 existing ignored, 0 measured**이다.

| 검증 범위 | 통과 | 기존 ignored |
|---|---:|---:|
| anchor-plane 동등성·경계·fallback | 9 | 0 |
| row-order family algebra·runtime 계약 | 17 | 1 |
| fused language 재사용·복구 | 4 | 0 |
| source-prefix 공급·target·소유권·필요조건 | 23 | 1 |
| stage profiler 샘플·경계 | 8 | 0 |
| Full6 실제 물리 pruning admission | 1 | 0 |
| harness OP7P7·post deadline·profile delta | 3 | 0 |

작은 유한 보드의 전수 비교는 시간 비교가 아닌 정확성 검증이다. 4×2의 256개
보드 ×7종 미노 ×5개 내장 회전 규칙에서 기존 scalar와 대조하고, 10×6의 실제
grounded target에서는 forward/reverse/hybrid/scalar 및 별도 cell oracle를
비교한다. 실제 32-pose handoff, ordered first-success, 첫 empty kick entry,
긴 imported kick 목록의 무손실 scalar fallback, 여러 word와 벽 경계를 포함한다.

family 검증에는 Union/Product/prefix/continuation, 비어 있지 않은 초기 보드의
Unknown, 15 O의 유효 drop/clear witness, 불가능 sibling 제거와 raw path count
보존, 기존 완료 serial/parallel 결과가 포함된다. 공급 상한의 독립 oracle는
**완화한 lock-subset oracle**이며 SRS 물리 검증기를 대신하지 않는다. fixed queue,
OP7P7, Hold-off, 지원하지 않는 source의 fallback 및 incomplete digest 상태도
확인했다. fused 경로는 warm buffer·dead root·invalid topology 이후 정상 복구를
검증했다.

`parallel,wasm-stage-profiling`의 release harness 빌드 및 non-profiling
`wasm32-unknown-unknown --lib --features parallel` check가 통과했다.
`rustfmt`와 `git diff --check`도 통과했다. CI를 실행한 결과는 아니다.

Google Rust style guide에 맞추어 rustfmt, 구체적인 module import, 빌린 immutable
입력과 명시적 fallback을 사용했다. unsafe, 새 dependency, 새 worker-retained
plane cache를 추가하지 않았다. 참고 문서는
[Google Rust Style Guide](https://google.github.io/styleguide/rustguide.html)이다.

## 첫 후보 비교와 dispatch 대조

첫 후보 source는 `5bae13f10dd2cefdbf00af2a897f8ad4b5298b5f`, binary SHA-256은
`23aaaaa6b9a610387487623a8fe408bfa4f2b6197fecb61766ce5c6ca92880d6`이다.
이 후보는 가족·공급·fused 변경과 32-pose reverse hybrid를 함께 켠다.
입력·feature·9-worker Auto·300초 후반 창은 기준과 같다.

| 관측 항목 | 기준 | 첫 hybrid 후보 |
|---|---:|---:|
| family 준비 경계 | 111.127824157 s | 110.637577801 s |
| 후반 취소 요청까지 | 300.000187793 s | 300.000143215 s |
| 후반 joined return까지 | 300.802953793 s | 300.426377832 s |
| emitted candidates, exact owner 합계 | 31,502,265 | 30,119,655 |
| row-order prune 작업 수, exact | 143,517,300 | 143,573,394 |
| source-prefix checks, exact | 171,153,515 | 163,139,914 |
| source-prefix prune 작업 수, exact | 15,054,771 | 13,276,684 |
| BuildOrder graph nodes, weighted | 901,753,501 | 925,903,398 |
| lock queries, weighted | 745,393,757 | 775,195,362 |
| generated states, weighted | 13,547,401,578 | 11,873,559,551 |
| BuildOrder 누적 sampled elapsed spans | 1,099.156113155 s | 1,130.915630506 s |
| coverage 누적 sampled elapsed spans | 208.666824684 s | 222.611963415 s |
| 후반 표본 RSS peak | 6,121,734,144 B | 6,176,243,712 B |
| whole-process VmHWM | 6,121,111,552 B | 6,175,498,240 B |

후보의 emitted 수는 4.39% 적지만 graph nodes는 2.68%, lock queries는 4.00%
많다. generated states는 12.36% 적으며, 이것은 anchor word 연산 수나 CPU 명령 수를
측정한 값이 아니다. sampled elapsed span도 프로세스의 CPU-time counter가 아니다.
후반 RSS 표본 peak는 약 51.98 MiB(0.89%) 높으므로 이 실행을 메모리 감소로
보고하지 않는다.

emitted는 verifier에 넘긴 후보 수라 취소 시 미완료 후보도 포함할 수 있다.
pruned_subtrees는 제거한 task/subtree 횟수이며 그 안의 leaf 수가 아니다. 두
prune counter를 더해 raw family paths에 대한 완료 비율을 만들 수 없다.
candidate projection·BuildOrder·coverage의 work와 invocation은 weighted sample이고,
stage-defined라는 단위명이 exact를 뜻하지 않는다. geometry의 별도 owner totals만
정확한 합계다.

가족 가지치기는 처리하는 후보 집합을 바꾸고 병렬 스케줄에 따라 도달한 prefix도
달라진다. 따라서 첫 후보만으로 reverse kernel의 단독 이득·회귀를 확정하거나
완료 Full6의 개선율을 주장하지 않는다. 동일한 family/source/fused 구현에서
reverse hybrid만 끈 `94f7bd09bb63661d59806829188ef8175cf7129d`를 별도 대조한다.

첫 후보 receipt `1795387159191910478-7-runtime`은 child exit 1, supervisor
termination 없음, memory pressure event 0, automatic retry 없음, 종료 후
descendant 0이다. supervisor 표본 peak는 6,176,018,432 B다.

### 같은 family 변경을 유지한 scalar 대조

대조 source는 위 `94f7bd09`, binary SHA-256은
`e99713def5843e834778d9ef68e2c6c5229d2a9ede44c7beebf15585833ac030`이다.
첫 후보와 제품 코드의 차이는 `HYBRID_REVERSE_DEFAULT` 한 줄이다.

| 관측 항목 | hybrid off 대조 | 첫 hybrid on 후보 |
|---|---:|---:|
| family 준비 경계 | 112.114767916 s | 110.637577801 s |
| 후반 취소 요청까지 | 300.002745469 s | 300.000143215 s |
| 후반 joined return까지 | 301.126533111 s | 300.426377832 s |
| emitted candidates, exact | 29,750,417 | 30,119,655 |
| graph nodes, weighted | 910,767,654 | 925,903,398 |
| lock queries, weighted | 761,934,562 | 775,195,362 |
| generated states, weighted | 13,851,745,783 | 11,873,559,551 |
| BuildOrder 누적 sampled elapsed spans | 1,132.222810705 s | 1,130.915630506 s |
| coverage 누적 sampled elapsed spans | 200.335253135 s | 222.611963415 s |
| 후반 표본 RSS peak | 6,159,044,608 B | 6,176,243,712 B |
| whole-process VmHWM | 6,158,266,368 B | 6,175,498,240 B |

hybrid의 emitted 수는 대조보다 1.24%, lock queries는 1.74% 많고,
generated states/lock는 18.179705에서 15.316861로 약 15.75% 줄었다.
BuildOrder 누적 span은 0.12% 작아 거의 같으며, span/lock는 약 1.82% 작다.
후반 RSS 표본 peak는 약 16.40 MiB(0.28%) 높다. 이 실행 쌍에서 가중
generated_states/lock가 낮게 관측됐지만, 큰 시간 개선이나 범용적인 개선율을
확정하지 않는다.
동일한 source-prefix 코드의 가중 span이 약 24% 다르다는 점도 후보 구성과
sampling 변동을 함께 고려해야 하는 이유다.

대조 receipt는 `1795387735878694449-7-runtime`이다. 세 실행 모두 full domain을
완료하지 않은 협력 취소이며 원본 family count는 같다.

### 준비 비용을 줄인 후속 dispatch

후속 구현은 다음 scalar pose의 기존 sky/proven-exit 확인을 집합 전환 앞으로
옮긴다. 즉시 성공이면 기존대로 `visited_state_count=cursor+1`이며, 실패하면
cursor를 아직 증가시키지 않아 현재 pose가 pending에서 빠지지 않는다.

실제 pending pose 수를 P, pending에 등장하는 회전 수를 R, 회전당 실제 word
수를 W, 선택한 고정 word-buffer capacity를 C(2/8/64)라 할 때 adapter를
만들기 전에 다음 조건을 확인한다.

\[
P\ge\max(8,4WR,2C).
\]

이는 reachability 판정을 바꾸는 가지치기가 아니라 구현 선택 휴리스틱이다.
C는 처리한 prefix 길이가 아니다. 이미 처리한 prefix의 재인코딩 비용은 남으며,
이 식이 전체 시간이나 후속 word 작업의 상한을 보장하지 않는다. 작은 P는
길이만 보고 거절하고, 모든 회전에 충분한 P는 추가 순회 없이 승인한다. 그
사이에서만 pending suffix를 읽어 실제 R을 센다.

희소하다는 이유로 거절한 경우만 scalar 처리 32→64→128…에서 다시 확인한다.
표현 범위·adapter·kernel 자체가 거절하면 해당 query에서 반복 시도하지 않고
기존 scalar를 계속한다. 새 retained field나 heap은 없고 query-local usize
하나를 추가한다. 최종 기본 적용 판단은 이 후속 구현의 기능·계측 결과에 따른다.

### 과거 메모리 최적화와의 경계

기존 `accept absorption`은 이번에 켜지 않았다. 이전
[P7P4 독립 ABBA](placement-dependency-prototype-2026-11-20.md)의 빈 4L / Jstris180 /
8-worker strict-prefix 네 표본은 79.850 / 89.438 / 85.349 / 76.072초였고,
시간 중앙값은 +12.10%, 표본 peak는 −55.5 MiB였다. 이는 Full6에서 측정한
개선율이나 회귀율이 아니다. 당시 합집합 메모 압축만 채택한 결정을 유지한다.
상태 방문이나 메모리만 줄었다는 이유로 과거에 느렸던 옵션을 제품에 다시 켜지 않는다.

## 후속 구현 검증

밀도 dispatch source `a39ff16bed328fcf08f6d2cb19bac3ea9da85878`는
`FULL_FORWARD_DEFAULT=false`, `HYBRID_REVERSE_DEFAULT=true`로 고정해 확인했다.
후속 anchor-plane 검사 **10 passed**와 기존 reverse/proven-exit/scoring 관련
검사 **21 passed, 1 existing ignored**가 통과했다. 첫 checkpoint의 33번째
즉시 sky exit와 실제 dense handoff 발생 assertion도 실행했다.

앞선 최종 기능 검증과 후속 묶음에서 중복 test 이름을 제거하면 **87 passed,
3 existing ignored**다. 단일 전체-suite 실행이나 CI의 통과 수로 표현하지 않는다.
후속 로그는 `density-functional-tests.log`, receipt는
`1795388316320040891-5-runtime`이다.

후속 native release harness 및 non-profiling WASM library check도 통과했다.
로그는 `density-release-build.log`, receipt는 `1795388396744674215-5-runtime`이다.
해당 binary SHA-256은
`c04af6eb95766c63d6375fe03470aff9c92f5469a0c8bffce9c109dc3168524d`이다.

### P1P7P7 후속 계측의 비정상 종료

후속 density 후보의 P1P7P7 실행은 family 준비 113.663060317초 이후 진행했으나
정상적인 300초 후반 취소·join 전에 `SIGKILL(9)`, exit 137로 끝났다.
최종 `full6-density.json`은 0 bytes여서 **정상 비교 표본에서 제외한다**.
중간 worker checkpoint를 합쳐 완성된 후반 profile로 복원하지 않는다.

receipt `1795388518821465892-7-runtime`의 기록은 다음과 같다.

| 항목 | 관측 |
|---|---|
| manager duration | 365,710 ms |
| child termination | Unix signal 9, core dump 없음 |
| supervisor timeout 설정 | 900 s |
| supervisor termination reason | `null` |
| memory pressure event | 0 |
| sampled process-group peak | 6,022,774,784 B |
| OOM-kill delta / observed finite cgroup limit / kernel peak | 모두 `null` |
| automatic retry | false |
| 종료 후 descendant | 0 |

이 자료로 OOM이나 timeout을 원인으로 단정할 수 없으며, 관측 event가 없다는
이유로 OOM 부재를 증명할 수도 없다. 짧아진 관측 창의 peak를 앞선 300초 창과
비교해 메모리 개선으로 보고하지 않는다.

P1P7P7를 자동 재시도하지 않는다. 사용자가 명시한 대안 OP7P7로 후속 비교를
옮기되 binary·9-worker Auto·`benchmark-search` profile·후반 300초·외곽 900초를
유지한다. 입력은 빈 10×6/SRS+/15 draws/15 locks/CountUnique이고, 원본 큐 수는
25,401,600이다. 자원 한도나 worker 수를 바꾸어 재시도하지 않는다.

## OP7P7 후속 비교와 최종 기본값

같은 OP7P7 요청에서 control `94f7bd09`와 density `a39ff16b`를 각각 실행했다.
두 실행의 raw family는 17,908,756,157,184 paths, 30,659,028 nodes로 같다.
두 군 모두 후반 300초에서 정상적인 협력 취소와 joined result를 기록했다.

| 관측 항목 | scalar control | density 후보 |
|---|---:|---:|
| family 준비 경계 | 71.508344547 s | 79.967524461 s |
| 후반 stop 요청까지 | 300.000148980 s | 300.000148391 s |
| 후반 joined return까지 | 300.129119323 s | 300.134353408 s |
| emitted candidates, exact | 14,843,341 | 14,008,868 |
| row-order prune 작업 수, exact | 61,630,533 | 58,287,457 |
| source-prefix checks, exact | 77,183,398 | 72,844,176 |
| source-prefix prune 작업 수, exact | 2,813,563 | 2,651,279 |
| graph nodes, weighted | 1,123,169,750 | 1,050,589,654 |
| lock queries, weighted | 1,749,990,775 | 1,640,832,375 |
| generated states, weighted | 27,203,142,471 | 25,439,727,768 |
| generated states / lock | 15.544735 | 15.504160 |
| BuildOrder 누적 sampled elapsed spans | 1,712.905557461 s | 1,714.601900273 s |
| BuildOrder span / lock | 978.808 ns | 1,044.959 ns |
| coverage 누적 sampled elapsed spans | 256.250185489 s | 228.038127004 s |
| 후반 표본 RSS peak | 5,105,053,696 B | 5,091,254,272 B |
| whole-process VmHWM | 5,104,267,264 B | 5,090,578,432 B |

density의 emitted 수는 5.62%, lock queries는 6.24% 적고, states/lock 감소는
0.26%다. BuildOrder span은 거의 같은 반면 span/query는 6.76% 높다. 후반 RSS
peak는 약 13.16 MiB(0.27%) 낮다. 같은 family/source/fused 코드를 유지한 비교지만
방문 후보 identity와 환경이 완전히 고정된 replay는 아니다. 준비 시간도 약
11.83% 길어져 환경 변동을 배제할 수 없다. 이를 확정적인 kernel 회귀율로
보고하지 않으며, 작은 메모리 차이를 속도 비용과 바꾸어 기본 승격하지도 않는다.

두 receipt는 각각 `1795389025018140882-5-runtime`,
`1795389418722320294-5-runtime`이다. child exit 1 / `Cancelled` /
`complete=false`, memory pressure event 0, automatic retry 없음,
supervisor termination reason null, 종료 후 descendant 0이다. supervisor
표본 peak는 각각 5,104,803,840 B와 5,091,209,216 B다.

최종 `f557ef62`에서는 두 anchor 기본값을 모두 false로 되돌렸다. 정확한 집합
커널과 differential fixture는 연구·명시적인 진단용으로 보관한다. 기존 제품
경로는 scalar를 사용한다. 가족 first-lock 조건, 일반화한 source capacity,
fused 쓰기 제거, phase 계측은 유지한다. 이후의 문서 커밋은 이 code 내용을
바꾸지 않는다.

기본값을 끈 최종 `f557ef62`에서 anchor 10개와 기존 reverse 21개 기능 검사를
다시 통과했고, non-profiling native library와 WASM library check도 통과했다.
receipt는 `1795390034862202407-5-runtime`, 로그는
`final-dispatch-verification.log`다. 같은 테스트의 재실행을 새로운 고유 case로
더하지 않으며, 전체 확인한 고유 case 수는 87 passed / 3 existing ignored다.

## CountUnique와 전체 큐 coverage의 남은 결합

실제 물리·원본 source·Hold·EOF를 함께 만족하는 관계를 E(C,q)라고 하면,
서로 다른 두 집합을 구분해야 한다.

\[
S=\{\operatorname{id}(C):\exists q\in U,\ E(C,q)\},\qquad
G=\{q\in U:\exists C,\ E(C,q)\}.
\]

CountUnique는 |S|를 센다. 큐×해법 쌍이나 실행 경로 multiplicity를 세지 않는다.
그러나 현재 일반 PC API는 G의 bitmap, covered count, coverage 값 및 completeness도
함께 반환한다. `PcChanceEvidencePolicy::Disabled`는 제품 전용 증거 보관 정책이고,
`PcSolutionProbabilityPolicy::Omit`은 해별 mapping/확률 출력을 생략한다. 두 옵션만으로
G의 계약이 사라지지 않는다. harness의 `record_result`도 coverage bitmap 길이·
popcount·원본 ordinal hash와 완료 상태를 확인하도록 되어 있다.

관련 계약과 구현은 [CountUnique 의미](dependency-cycle-queue-exactness-2026-11-23.md),
[Include/Omit 경계](placement-graph-p7p4-2026-11-20.md),
[symbolic membership의 미구현 범위](graph-only-probability-2026-11-20.md),
`search_problem_fields.rs`, `result.rs::process_admitted_candidate_ranked`,
`parallel_worker.rs::WorkerAggregate::observe`, `result.rs::build_result`에 있다.

따라서 후보의 존재 판정 자체는 Boolean이어도 되지만, 현재 요청의 모든 coverage를
Boolean 하나로 바꾸면 공개 결과의 G가 빠진다. MDD라는 자료구조나 매 후보의 완전한
root 생성이 계약 자체인 것은 아니다. 이미 알려진 coverage에 새 정보가 필요 없는
범위 또는 **global coverage를 명시적으로 요청하지 않는 별도 계약**에서는 실제
source/Hold/terminal 조건을 만족하는 membership 계산으로 줄일 여지가 있다.
현재 `MembershipOnly`는 concrete 단일 pattern에 한정돼 있고, 이 실험의 symbolic
target은 그 조건을 충족하지 않는다.

이번 작업은 알려진 100% 확률을 다시 증명하거나 이를 새 후보의 양성 권한으로
사용하지 않았다. 다음 구조 개선에서는 G를 요구하지 않는 요청을 명시적으로
분리하여 이미 불필요하다고 정한 출력의 계산·보관 비용을 줄이는 것이 우선이다.
기존 요청의 coverage를 몰래 생략하거나 비어 있는 집합을 계산 완료로 보고해서는
안 된다.

## 후속 구현 경계

- [ ] 수정 파일: `crates/clearra-problem/src/search_problem_fields.rs`,
  `crates/clearra-core-executor/src/backend/wasm_cpu/result.rs`,
  `crates/clearra-core-executor/examples/full6l_bench.rs`; 구현 내용: global coverage
  생략 여부를 명시하는 결과 계약과 진단 모드를 정의하고, 기존 요청의 bitmap·
  completeness 의미를 유지한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs`,
  `standard_bag_coverage.rs`, `geometry.rs`; 구현 내용: symbolic source/Hold/EOF의
  정확한 존재성과 필요한 대표 witness만 계산하는 경로를 공통 전이 위에 만든다.
  단순 geometry 양성을 원본 큐의 양성으로 사용하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs`,
  `geometry_row_order.rs`, `buildup.rs`; 구현 내용: family derivative와 실제 board·
  original row frame을 묶어 같은 잔여 문제를 공유한다. 현재 Append/Union/Product
  식을 canonical ZDD로 간주하거나 raw path count를 CountUnique로 합산하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/examples/full6l_bench.rs`,
  `performance/search_stage_profiler.rs`; 구현 내용: 큰 입력에서 수집한 동일한
  query 집합의 제한된 replay와 준비/word 작업 계수를 분리해 dispatch 손익을
  판정한다. 극소 입력의 시간 벤치마크로 대체하지 않는다.

별도 사용자 승인 전까지 CI를 실행하거나 기다리지 않는다. main 승격과 배포도
수행하지 않았다. 기존 준비된 worker 즉시 투입·Auto 논리 CPU·NUMA fallback·
Chromium host-yield 정책은 유지했으며, 이번 native 계측을 다중 NUMA나 Chromium
실측 결과로 확대하지 않는다.

## 원본 근거 위치

raw 출력은 `_local/artifacts/post-generation-set-reuse-20261123/`에 보존한다.
이 문서에는 검토한 수치와 결론만 기록한다.

| 근거 | 파일 |
|---|---|
| P1 기준 | `full6-baseline.json`, `full6-baseline.stderr` |
| P1 첫 hybrid | `full6-candidate.json`, `full6-candidate.stderr` |
| P1 scalar 대조 | `full6-scalar-control.json`, `full6-scalar-control.stderr` |
| P1 density 비정상 종료 | `full6-density.stderr`; JSON은 0 bytes |
| OP scalar 대조 | `op7p7-scalar-control.json`, `op7p7-scalar-control.stderr` |
| OP density | `op7p7-density.json`, `op7p7-density.stderr` |
| 최초 최종 기능 검증 | `final-functional-tests.log` |
| density 후속 기능 검증 | `density-functional-tests.log` |
| 기본값 최종 확인 | `final-dispatch-verification.log` |
| release·WASM 빌드 | `candidate-build.log`, `density-release-build.log` |

초기 PATH 해석 실패, 이전 fixture 실패와 슬롯 충돌 로그는 보존했으며 성공한
검증 수에 넣지 않았다. OOM에 맞춰 자원을 바꾸는 자동 재시도는 하지 않았다.

## GitHub에서 확인할 수 있는 동일 source tree

측정 binary가 기록한 source SHA는 로컬 원본 Git commit이다. GitHub로 옮긴
commit은 작성 metadata가 다르지만 **각 전체 Git tree SHA가 원본과 일치함을
검사**했다. 측정 source를 새 SHA에서 다시 측정한 결과로 바꾸어 쓰지 않는다.

| 역할 | 로컬 source | 동일 tree의 GitHub commit |
|---|---|---|
| 기준 계측 | `212c5ccfe308945ad1f134d91b1bde492849abf1` | [a0c7ac06](https://github.com/daejunnom/Clearra/commit/a0c7ac06ddb59252ae5fb8ae4a1693ca68df0a6b) |
| 첫 hybrid | `5bae13f10dd2cefdbf00af2a897f8ad4b5298b5f` | [9ec7910f](https://github.com/daejunnom/Clearra/commit/9ec7910f46cf4ee0df2a7eb62ef3a2925b5c4ebe) |
| scalar control | `94f7bd09bb63661d59806829188ef8175cf7129d` | [951104a8](https://github.com/daejunnom/Clearra/commit/951104a8320a1408d1c038d87eb8c2e5e2cefb14) |
| density 후보 | `a39ff16bed328fcf08f6d2cb19bac3ea9da85878` | [cc530654](https://github.com/daejunnom/Clearra/commit/cc530654c1f72fc04987e6e181146f604c885929) |
| 최종 기본값 off | `f557ef62f75df27026b27fb41c93134beee46ed2` | [4f21d60b](https://github.com/daejunnom/Clearra/commit/4f21d60b8ec62e21460bd17f7a00012a2a8d222f) |

최종 code tree는 `29e85fb16bf318f365da530d99a4862f79870497`이다. 이 보고서와
이전 작업의 CI 중단 안내를 더한 검토 브랜치는
[`codex/post-generation-set-reuse-20261123`](https://github.com/daejunnom/Clearra/tree/codex/post-generation-set-reuse-20261123)이다.
이 브랜치는 현재 push workflow의 branch 조건에 해당하지 않으며, CI dispatch나
PR 생성은 하지 않았다.
