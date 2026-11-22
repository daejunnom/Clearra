# see-7 구조 개선과 전역 킥 가지치기 측정

작성일: 2026-11-22. 작업 브랜치: `codex/see7-quotient-pruning-20261122`.

**q23까지의 현재 판단:** 실제 8-lock의 완료 비교는 전체 원본 coverage가 같다.
q22의 cone 3은 완료했지만 bitmap은 이 fixture에서 더 비쌌다. q23 공유 생산자는
관측 179개와 증분 importer 5개, 총 **184개**의 서로 다른 focused 검증과 release를
마쳤다. 같은 바이너리의 shared 0/1 ABBA는 평균 **4.0258714005초 →
3.9106416619999997초**, 비율 **0.971377690185114**였으며 각 모드 두 표본의
기술 통계로 한정한다. q23 10-lock의 group 0/1은 모두 등록·freeze 뒤 정책의
공통 64M work 상한으로 미완료 종료했다. **11-lock q23은 생성 중 8M suffix 참조
상한으로 미완료 종료했다. 10/11-lock 완주는 아직 확인하지 못했다.**
§3.44–3.50에 검증·완료·실패를 구분한다. q24 ordered policy class와 q25 Full
multiset 인증은 후속 구현 중이며 아직 실행 결과로 채택하지 않는다.

아래 앞부분은 각 구현 단계의 이력이다. 후속 결과는 번호를 이어 추가하며,
최종 판단에는 같은 binary·같은 계약의 완료 비교만 사용한다.

이 기록에는 q2 전체 P7P4 see-7의 미완료 종료, P7P4 oracle의 Off/Current/Improved 8회 비교, q3의 실제 8-lock see-7 두 경로 완료, q4의 같은 바이너리 kind union 비교, q5의 8-lock source prefix 비교, q6의 8-lock boundary suffix 비교와 q9까지의 전체 P7P4 work-limit 실패가 포함된다. q2는 registration과 merge를 통과했지만 정책 평가의 초기 확장에서 work 상한으로 거절되었다. oracle의 같은 Current–Improved ABBA block에서는 Improved의 전체 검색시간 평균이 약 3.10% 낮았으며, 이 결과와 동일 판정의 중복 계산 제거를 근거로 현재 소스의 제품 기본을 Improved로 채택했다. q3의 8-lock board closure는 기존 factored 경로와 완전한 원본 coverage SHA가 같았지만, 정책 상태·검사가 늘어 총시간은 더 길었다. q4에서는 board 경로를 고정하고 revealed kind union만 켰을 때 같은 coverage로 233.053567468초에서 43.714876175초로 줄었다. q5의 8-lock source prefix는 같은 coverage를 보존하고 일부 producer 작업을 줄였지만, 42.104770599초 대 42.220748636초로 이 pair에서 총시간 개선은 관측하지 못했다. q6의 같은 바이너리 8-lock pair는 43.755515787초 대 40.142549954초였으며 전체 coverage SHA가 같았다.

전체 P7P4는 q4에서 suffix 상한, q5/q6에서 work 청구 상한으로 실패했다. q7은 전체 456,459 candidate factor 등록 뒤 freeze로 진입했지만 96.746755932초 후 work 상한으로 종료되었다. q9의 hybrid edge는 전체 등록과 freeze를 마친 뒤 정책 평가에 진입했으나, kind union의 추가 work 청구에서 총 107.122856232초 후 종료되었다. q9의 같은 바이너리 8-lock 네 arm은 모두 완료하여 원래 coverage SHA와 확률의 동등성을 확인했다. 순차 단일 표본이므로 전체 시간 차이를 안정적인 개선율로 해석하지 않는다. q9의 11-lock은 내부 단계와 원인을 확인할 수 없는 SIGKILL로 종료되었다. q10의 공급 개수 필터도 전체 P7P4에서 작동했지만 93.529510444초 후 같은 work 상한으로 미완료다.

q10의 같은 바이너리 8-lock supply multiset 0/1 ABBA 네 실행은 모두 완료했고 전체 원본 coverage SHA와 확률이 같았다. Rust comparator의 모드별 평균은 36.9037816145초와 10.6043791735초이며 비율은 0.2873521007758564다. 이 fixture에서는 총시간·정책 상태·메모리가 줄었지만 charged kind-union work는 늘었다. 따라서 이 결과를 공통 64M work 상한의 여유 증가나 10/11-lock 완주로 해석하지 않는다. 10/11-lock 완주는 아직 pending이다. 수치의 평균·범위·변화율은 Rust microbenchmark summary, `placement_graph_bench --compare`, 독립 Rust 관측 비교기의 출력에서 가져왔으며 다른 스크립트로 통계를 다시 계산하지 않았다.

q11의 prefix mask는 같은 바이너리 P7P4에서 불가능한 prefix 5,913,906건의 방문을 피했다. helper 구성·조회 work도 청구했으며 mask 1과 0 모두 전체 등록·freeze 후 정책 work 상한으로 미완료다. 두 실패 종료시간으로 속도 개선을 주장하지 않는다. 이어진 8-lock ABBA 네 실행은 전체 coverage가 같았지만, Rust comparator의 mask 1/0 평균 비율은 1.0077841081246686으로 이 비교에서 총시간 개선을 관측하지 못했다. prefix mask 제품 기본은 off를 유지한다.

q12 subset import와 q13 공개 전 memo를 통합하고 focused 검증을 마쳤다. 첫 q13 실행은 짧은 source SHA가 기록되어 비교기의 출처 gate를 통과하지 못했으므로 예비 자료로 보존한다. 전체 source SHA를 기록한 q13b의 정식 8-lock 다섯 arm은 완료·동등성 gate를 통과했다. 공개 전 memo ABBA는 평균 10.3844852355초에서 11.5150504765초로 느려져 기본 off를 유지한다. subset import는 중앙 kind arena를 121,268개에서 45,901개로 줄였지만, 시간 대조는 off 한 표본과 on 두 표본으로 ABBA가 아니므로 속도 개선을 단정하지 않는다. q13b P7P4도 전체 등록·freeze 뒤 정책 work 상한으로 77.472324947초에 미완료 종료했다.

후속 q14는 실제 fresh-bag 공급에 대한 후보 존재성 증명으로 후보별 oracle product를 생략하는 좁은 producer를 구현했다. 두 lookahead projection이 모두 false인 조건을 유지하며, 관측 120개·suffix 3개 focused 검증과 release build를 마쳤다. 이 검증은 구현 회귀 근거이며 전칭 증명은 [B15](see7-quotient-pruning-proof-2026-11-22.md)의 별도 논증을 따른다. 정식 8-lock source-existence ABBA는 전체 coverage·accepted 후보 수를 보존했지만 평균 비율 1.0351020300827798로 총시간 개선을 관측하지 못했다. q14 P7P4는 전체 456,459 후보의 등록·freeze 후 33.97099872초에 정책 work 상한으로 미완료 종료했다. 11-lock도 578.629917026초 후 worker producer의 physical node 상한으로 거절됐으며 최종 확률은 없다.

q15의 source-qualified prefix filter는 `55167107eee0fa66a880a355b430cb45f5d89979`에서 관측 124개·직접 filter 7개 focused 검증과 release build를 마쳤다. 같은 바이너리 8-lock filter 0/1 ABBA는 전체 coverage·확률·accepted 후보 수를 보존했고, Rust comparator의 on/off 평균 비율은 0.9344308135557023이었다. 전체 P7P4도 등록·freeze를 통과했지만 35.667625777초에 정책 work 상한으로 미완료다. 11-lock은 아홉 worker의 생산 종료 후 coordinator merge/import까지 도달했으나 578.288882526초에 physical node 상한으로 거절됐다. 8-lock의 비용 감소를 10/11-lock 완주로 해석하지 않는다.

같은 q15 P7P4에서 supply multiset만 0으로 바꾼 대조도 freeze 후 SIGKILL로 종료되어 완료 확률을 얻지 못했다. 또한 독립 Rust 추출기는 q15 11-lock의 관측 producer work 하계를 **77,863,423**으로 확인했다. 이 실행에서는 정책 평가나 merge 뒤 표현만 줄여서는 기존 64M work 상한을 해결할 수 없으며, producer 단계의 실제 작업 또는 공유 구조도 바뀌어야 한다. 다른 알고리즘의 불가능성이나 상한 완화를 뜻하는 결론은 아니다.

q16 `86aa5b8e255c53636bfacc21a04c4efa706c2759`의 보드별 multiset group index는 관측 focused **129개**와 release build를 마쳤다. 정확한 root 집합 동치는 [B17](see7-quotient-pruning-proof-2026-11-22.md)의 별도 논증을 따르며 테스트 수로 대체하지 않는다. 실제 8-lock ABBA는 전체 coverage·후보 수를 보존했지만 평균 비율 **1.0229530030866167**로 총시간 개선을 관측하지 못했다. Kind-union work는 줄었으나 P7P4는 **38.250364388초**에 공통 work 상한으로 미완료다. Group 기본은 0을 유지한다. q16은 q15 producer를 바꾸지 않으므로 하계가 이미 확인된 11-lock을 다시 600초 실행하지 않았다.

q17 `7108df03b50df28c8a04eae7dd71f1e2f22dd76d`는 정확히 계산한 incumbent와 원래 정보집합의 성공 개수 상계로 불리한 행동 계산을 중단한다. [B18](see7-quotient-pruning-proof-2026-11-22.md)의 별도 논증과 관측 focused **135개** 통과·release 완료를 기록했다. 같은 바이너리 8-lock branch-bound ABBA는 전체 원본 coverage·확률을 보존하며 평균 **9.7012302315초 → 3.9884741034999998초**, 비율 **0.411130754380963**을 기록했다. P7P4의 group 0/1은 모두 전체 등록·freeze 후 정책 kind-union에서 공통 64M work 상한으로 종료했다. 이 정책 개선을 producer 비용 감소나 10/11-lock 완주로 해석하지 않는다.

## 1. 작업 상태와 현재 판단

- [x] 수정 파일: `reachability.rs`, 구현 내용: 전역 ordered-kick 판정을 Off/Current/Improved의 같은 바이너리 실험 축으로 연결했다. Improved는 같은 E2 판정의 count/fill 중복 계산을 재사용한다. 반복 oracle 비교 후 현재 소스의 제품 기본을 Improved(2)로 채택했다.
- [x] 수정 파일: `reachability_symbolic_target.rs`, `reachability_symbolic_prefix.rs`, `symbolic_prefix_parallel.rs`, 구현 내용: 역방향 후보 검사와 원래 prior guard 검사를 분리해 기록하고, Current/Improved의 동일 CSR과 동일 query work를 확인했다.
- [x] 수정 파일: `reachability_global_kick_pruning_bench.rs`, 구현 내용: 유한 16개 측정 sample과 Rust summary로 생성비용·query 비용·작업량·일시 메모리를 분리했다.
- [x] 수정 파일: `placement_graph_bench.rs`, 구현 내용: 일반 제품 oracle P7과 P7P4의 3모드 8회씩을 완료하고 Rust comparator로 보고된 결과 동등성을 확인했다. P7P4 시간 해석은 두 ABBA block을 구분한다.
- [x] 수정 파일: `queue_observation_factored_actions.rs`, 구현 내용: q1의 공개 후 Frontier 표현을 적용하고, 전체 see-7 실패 지점을 registration의 work 상한으로 좁혔다.
- [x] 수정 파일: `queue_observation_factored_actions.rs` 및 연결 소비자, 구현 내용: q2의 productive relation 공유와 도달하지 않는 reveal binding 생략을 구현하고 release library 검증을 완료했다.
- [x] 수정 파일: `observation_dependency_bench.rs` 및 q2 구현, 구현 내용: q2 전체 P7P4 see-7의 종료 상태와 resource 거절 단계를 확인했다. registration/merge 이후 초기 정책 확장에서 실패했으며 완료 확률은 없다.
- [x] 수정 파일: `queue_observation_board_actions.rs`, `queue_observation_policy.rs` 및 연결 소비자, 구현 내용: q3의 실제 8-lock/9-draw see-7을 factored/board 두 경로로 완료하고 동일 원본 coverage SHA를 확인했다. board 경로의 총시간 악화와 정책 상태 증가도 함께 기록했다.
- [x] 수정 파일: 관측 kind union producer와 소비자, 구현 내용: q4의 같은 바이너리·board mode 1에서 kind union 0/1의 실제 8-lock 완료 결과를 비교했다. 전체 원본 coverage SHA를 유지하며 정책 상태·검사 수와 총시간이 줄었다.
- [x] 수정 파일: `queue_observation_board_actions.rs` 및 연결 소비자, 구현 내용: q4 전체 P7P4에서 board suffix root 8,000,000개 상한으로 거절된 상태를 기록했다. 완료 확률과 개선율은 없다.
- [x] 수정 파일: `buildup.rs`, `queue_observation_board_actions.rs` 및 연결 소비자, 구현 내용: q5의 첫 bag 공급으로 불가능한 hidden prefix를 제한하는 경계를 구현하고 관측 테스트 78개와 release build를 완료했다. 실제 8-lock source prefix 0/1의 coverage는 같았지만 이 pair의 총시간 개선은 관측하지 못했다.
- [x] 수정 파일: 이 문서, 구현 내용: q5 P7P4의 source prefix mode 1이 incoming shard merge의 work 사전 청구에서 거절된 것을 기록했다. 가지치기 적용 횟수와 최종 미완료 상태를 분리했다.
- [x] 수정 파일: 관측 board producer와 이 문서, 구현 내용: q6의 H 경계 suffix binding만 보유하는 변경을 8-lock boundary 0/1의 같은 바이너리 비교로 확인했다. 전체 원본 coverage SHA가 같았고 한 pair의 총시간 평균 비율은 0.9174283340507797이었다. 전체 P7P4는 여전히 work 상한으로 거절되었다.
- [x] 수정 파일: 관측 board producer와 이 문서, 구현 내용: q7 edge closure는 전체 456,459 candidate factor 등록 뒤 freeze의 edge 재구성으로 진입했으나 work 상한에서 종료되었다. 등록 단계 통과와 최종 탐색 미완료를 구분했다.
- [x] 수정 파일: `queue_observation_policy.rs`, `queue_observation_board_actions.rs` 및 이 문서, 구현 내용: q8 수집 단축·q9 hybrid edge를 반영한 `dcf06bc`에서 기본 관측 테스트 84개, parallel/local A/B 관측 테스트 86개와 release build를 완료했다. 전체 P7P4는 등록·freeze 뒤 kind union의 work 상한에서 실패했으며 완료 확률은 없다.
- [x] 수정 파일: 이 문서와 독립 관측 비교기, 구현 내용: q9의 8-lock edge 0/1/2와 collection 0/1 네 arm을 완료하고 기존 비교 출력 v6에서 원래 coverage와 확률의 동등성을 확인했다. 각 arm 한 번의 순차 실행이라는 한계, freeze와 policy 시간, owner 계측과 sampled RSS를 구분했다.
- [x] 수정 파일: 이 문서, 구현 내용: q9 11-lock의 exit 137/SIGKILL과 0-byte JSON을 기록했다. 내부 단계·OOM 여부를 특정할 증거가 없으며 600초 timeout으로 표시하지 않는다.
- [x] 수정 파일: 공급 개수 필터와 이 문서, 구현 내용: q10 `d17c26e`에서 92개 focused 검증과 release build를 완료하고 P7P4의 실제 checked/pruned root 및 certificate 비용을 확인했다. 정책 work-limit 실패를 완주나 speedup으로 해석하지 않는다.
- [x] 수정 파일: 이 문서와 기존 관측 비교기, 구현 내용: q10의 같은 바이너리 8-lock supply multiset 0/1을 ABBA로 두 번씩 완료하고 원본 coverage SHA·확률·완료 gate의 동등성을 확인했다. 총시간·메모리 감소와 charged kind-union work 증가를 함께 기록했다.
- [x] 수정 파일: prefix mask producer·소비자와 이 문서, 구현 내용: q11 `55ec4b7`의 관측 focused 101개 통과와 release build 완료를 기록했다. 같은 바이너리 P7P4 mask 1/0 모두 정책 work 상한에서 미완료이며, 방문 생략과 helper 청구를 분리했다. 제품 기본은 prefix mask 0이다.
- [x] 수정 파일: 이 문서와 독립 관측 비교기, 구현 내용: q11의 같은 바이너리 8-lock prefix mask ABBA를 완료하고 원래 coverage·확률·완료 gate의 동등성을 확인했다. 총시간 개선은 관측하지 못했으며 prefix mask 기본 off를 유지했다.
- [x] 수정 파일: subset import·공개 전 memo producer와 연결 소비자, 구현 내용: q12 `cd08d4b`와 q13 `c3b8adf`를 `abddcccc865472b0cb0c7b4b0ebb7da43dcf984b`에서 연결하고 관측 110개·subset import 5개 focused 검증을 각각 완료했다.
- [x] 수정 파일: 이 문서, 구현 내용: source SHA를 짧게 기록한 첫 q13 실행의 출처 gate 실패를 보존하고 정식 시간 비교에서 제외했다. 전체 SHA를 기록한 q13b의 release build를 완료했다.
- [x] 수정 파일: 이 문서와 독립 관측 비교기, 구현 내용: q13b의 정식 8-lock 다섯 arm에서 완료·동등성 gate를 통과했다. 공개 전 memo ABBA의 시간 악화와 기본 off 유지, subset import의 중앙 arena 축소 및 반복 배치가 다른 시간 비교의 한계를 기록했다.
- [x] 수정 파일: 이 문서, 구현 내용: q13b P7P4의 전체 등록·freeze 후 정책 work-limit 실패를 원본 계측과 receipt로 확인했다. 줄어든 중앙 arena와 남아 있는 공통 work 병목을 분리했다.
- [x] 수정 파일: fresh-bag 존재성 판정·suffix producer와 연결 소비자, 구현 내용: q14 `a4bbd22` 및 명시적 strict-prefix 실험의 선행 dispatch를 보존하는 `188e583b7e59767d3f95d88ad430585d26cf0d75`를 통합했다. 최초 테스트 import 컴파일 오류를 수정한 뒤 관측 120개·suffix 3개 검증과 release build를 완료했다.
- [x] 수정 파일: 독립 관측 비교기와 이 문서, 구현 내용: v11 입력의 source-existence 축·typed source metadata와 새 필수 계측을 추가해 비교 출력 v10을 컴파일했다. 기존 q13b 다섯 arm의 backward comparison은 실제 완료·동등성 gate를 통과했다.
- [x] 수정 파일: q14 producer·소비자와 이 문서, 구현 내용: 같은 바이너리 source-existence 0/1의 8-lock ABBA에서 완료·원본 coverage 동등성과 accepted 2,597개 보존을 확인했다. 총시간 개선은 관측하지 못했고 oracle 실행 진단값의 의도적인 0과 최종 coverage를 구분했다.
- [x] 수정 파일: 이 문서, 구현 내용: q14 P7P4가 전체 456,459개 후보 등록·freeze 뒤 공통 work 상한으로 거절된 최종 JSON·stderr·receipt를 기록했다. 실패시간 비율을 만들지 않았다.
- [x] 수정 파일: 이 문서, 구현 내용: q14 11-lock의 최종 JSON·stderr·receipt에서 worker producer의 physical node-limit 거절과 exit 1을 확인했다. 600초 timeout이나 OOM으로 표시하지 않고 부분 worker 계측과 전체 미완료를 구분했다.
- [x] 수정 파일: source-qualified prefix compiler와 연결 소비자, 구현 내용: q15 `55167107eee0fa66a880a355b430cb45f5d89979`의 관측 124개·직접 source-filter 7개 focused 검증 및 release build를 완료했다.
- [x] 수정 파일: 독립 관측 비교기와 이 문서, 구현 내용: v12 입력의 source-filter 축·필수 구조 계측을 추가한 비교 출력 v11을 컴파일하고 기존 q14 8-lock ABBA의 backward comparison 및 실제 v12 q15 ABBA의 완료·동등성·후보 수 진단을 통과했다.
- [x] 수정 파일: 이 문서, 구현 내용: q15의 8-lock source-filter ABBA에서 구조·정책 비용 감소와 평균 비율 0.9344308135557023을 기록했다. P7P4의 전체 등록·freeze 후 정책 work-limit 실패와 구분한다.
- [x] 수정 파일: 이 문서, 구현 내용: q15 11-lock은 아홉 worker 종료 뒤 coordinator merge/import에서 physical node 상한으로 거절됐다. q14의 worker producer 실패와 구분하고 최종 확률·timeout·OOM으로 표시하지 않았다.
- [x] 수정 파일: 독립 Rust 작업량 추출기와 이 문서, 구현 내용: q15 11-lock의 모든 worker 완료 기록 및 공통 131,072 accepted milestone에서 producer work 하계 77,863,423을 확인했다. 이 실행·표현의 하계와 최종 coverage·보편적 알고리즘 불가능성을 구분했다.
- [x] 수정 파일: 이 문서, 구현 내용: q15 P7P4 supply multiset 0 대조의 freeze 완료·SIGKILL·빈 JSON과 receipt를 기록했다. OOM·timeout·완료 시간 개선으로 해석하지 않았다.
- [x] 수정 파일: 독립 관측 비교기와 이 문서, 구현 내용: q16 v13 multiset-groups 축·필수 계측을 지원하는 비교 출력 v12의 컴파일, 기존 q15 backward comparison 및 실제 q16 ABBA의 완료·동등성·후보 수 검증을 통과했다.
- [x] 수정 파일: kind-union group index·연결 소비자와 이 문서, 구현 내용: q16 `86aa5b8e255c53636bfacc21a04c4efa706c2759`의 관측 129개·release build를 완료했다. 8-lock의 work 감소와 총시간 개선 미관측, P7P4의 부분 정책 평가 후 work-limit 실패를 기록하고 group 기본 0을 유지했다.
- [x] 수정 파일: 독립 관측 비교기와 이 문서, 구현 내용: q17 v14 branch-bound 축·네 필수 계측을 지원하는 비교 출력 v13의 컴파일, 기존 q16 backward comparison 및 실제 q17 ABBA 완료·동등성 gate를 통과했다.
- [x] 수정 파일: 관측 정책 branch-bound·연결 소비자와 이 문서, 구현 내용: q17 `7108df03b50df28c8a04eae7dd71f1e2f22dd76d`의 관측 135개·release 및 8-lock ABBA를 완료했다. B18 논증·유한 회귀·실제 비용 감소를 구분하며 P7P4의 group 0/1 정책 work-limit 실패를 함께 기록했다.
- [x] 수정 파일: q22/q23 producer·소비자와 이 문서, 구현 내용: q22의 cone/bitmap 별도 8-lock ABBA와 q23 공유 생산자의 8-lock ABBA를 완료하고 전체 원본 coverage·확률·완료 gate의 동등성을 확인했다. q23의 관측 179개·증분 importer 5개 focused 검증, release, 원래 resource 상한 및 실제 memory receipt를 기록했다.
- [x] 수정 파일: 이 문서, 구현 내용: q23 P7P4 group 0/1의 전체 456,459개 등록·freeze 후 정책 work-limit 실패를 기록했다. 초기 관측 일부의 완료 수를 전체 완료로 표시하지 않고 실패시간 비율을 만들지 않았다.
- [ ] 수정 파일: 후속 producer·소비자와 이 문서, 구현 내용: 원래 계약의 10/11-lock 완주를 달성한다. q15의 별도 source filter는 q14 실측에 적용되지 않았다.
- [ ] 수정 파일: 물리 관측 relation producer와 소비자, 구현 내용: 전체 P7P4 see-7을 정확한 원래 공급·관측 계약으로 완주한다.
- [x] 수정 파일: `placement_graph_bench.rs`, 구현 내용: P7P4 Improved 반복 실행을 완료하고 같은 바이너리·CPU의 Current 3/4회차와 Improved 1/2회차를 같은 ABBA block 안에서 비교했다.
- [x] 수정 파일: `reachability.rs`, 구현 내용: 같은 block의 전체 검색시간과 자원 결과를 확인한 뒤 Improved를 제품 기본으로 채택했다. q3 구조 개선 측정에서는 가지치기를 명시적으로 Current(1)에 고정해 별도 효과를 분리한다.

현재 확인한 구조적 효과는 분명하다. Current는 불가능한 역방향 간선을 제거해 query 검사를 줄이고, Improved는 같은 간선 집합을 만드는 판정 계산을 한 번 재사용한다. P7P4 oracle에서도 같은 block의 총시간 감소를 관측했지만 각 모드 2회로 안정적인 개선율이나 p95를 추정하지 않는다. 서로 다른 block의 Current 실행을 모두 섞은 평균은 주효과로 사용하지 않는다. baseline, q1, q2의 전체 see-7은 모두 미완료 오류로 끝났으며, 그 종료시간은 speedup 비교값이 아니다.

정확성 논증과 미적용 설계의 경계는 [see-7 몫·가지치기 증명](see7-quotient-pruning-proof-2026-11-22.md)을 따른다. 테스트와 해시는 그 논증의 구현 연결을 확인하는 증거이며 전칭 동치 증명을 대신하지 않는다.

## 2. 소스·바이너리·실행 계약

원시 artifact root는 `_local/artifacts/see7-quotient-pruning-20261122`다. 이 문서는 선택한 결과만 요약하며 원시 로그를 문서로 복사하지 않는다.

| 구분 | 소스 기준 | 이 문서에서 사용하는 실행 |
| --- | --- | --- |
| baseline | `c89b330bca8fb6c10e9c6d4752d375999c62b11a` | 전체 P7P4 VisibleSeven 실패 기록 |
| q1 | `966be124a3b69e6e59841ff602951662b144f5f5` | P7/P7P4 oracle 가지치기 비교, 전체 P7P4 VisibleSeven registration 실패 |
| q2 | `413addd6f9045027943432ab3545a63e7304f2c9` | 16-sample microbenchmark, release library 검증, 전체 VisibleSeven 초기 평가 실패 |
| q3 | `14e8f72c8617d727421e71bf454eb89b909c7cfc` | 실제 8-lock/9-draw VisibleSeven, factored/board 각각 1회 완료 비교 |
| q4 | `9b6aa865080c2eabfe82573195dcfd476fa63cf2` | 실제 8-lock/9-draw의 kind union 0/1 완료 비교, 전체 P7P4 suffix-limit 실패 |
| q5 | `8ac99c4883bdf07e2bdeeae7ec30ab2580f6d02a` | 관측 테스트 78개 통과와 release build 완료, 8-lock source prefix 0/1 완료 비교, P7P4 work-limit 실패, 11-lock 제한 probe timeout; 10/11-lock 완주 pending |
| q6 | `eb55e5cf290cc30f063bdc89ec2d079cd3ce8eb4` | 관측 테스트 80개 통과, 8-lock boundary 0/1 완료 비교, P7P4 work-limit 실패 |
| q7 | `f689e67421670dd487168b1a4657adfc71fca3ce` | 관측 테스트 81개·core lib 1,104개 통과, P7P4 전체 factor 등록 후 freeze work-limit 실패 |
| q8/q9 | `dcf06bc726fdaf9fc1a0dfcf7d2ce65c5318e835` | 기본 관측 테스트 84개·parallel/local A/B 관측 테스트 86개 통과, release build 완료, P7P4 등록·freeze 완료 후 정책 kind union work-limit 실패; 8-lock 네 arm 완료·동등성 확인 |
| q10 | `d17c26e5f8d411f81c3fc7bd00f22959dbe772b7` | 관측 focused 92개 통과, release build 완료, P7P4 supply multiset 1의 정책 work-limit 실패; 8-lock 같은 바이너리 supply multiset 0/1 ABBA 네 실행 완료·동등성 확인 |
| q11 | `55ec4b711267253536008fec3b1e24bafff12045` | 관측 focused 101개 통과와 release build 완료; 같은 바이너리 P7P4 prefix mask 1/0 모두 정책 work-limit 실패, 8-lock ABBA 완료·동등성 확인·총시간 개선 미관측; prefix mask 기본 0 |
| q12/q13 통합, q13b 보관 바이너리 | `abddcccc865472b0cb0c7b4b0ebb7da43dcf984b` | q12 `cd08d4b`·q13 `c3b8adf`와 소비자 연결, 관측 110개·subset import 5개 통과; 정식 8-lock 다섯 arm 완료·동등성 확인, 공개 전 memo ABBA 악화; P7P4 정책 work-limit 실패 |
| q14 | `188e583b7e59767d3f95d88ad430585d26cf0d75` | `a4bbd22ae3f5c0f239b55330f39b5121775ca7d0`의 존재성 producer 및 명시적 strict-prefix 경로 보존; 관측 120개·suffix 3개 통과, release 완료; 8-lock source-existence ABBA 완료·동등성 확인·총시간 개선 미관측, P7P4 정책 work-limit 실패, 11-lock worker producer node-limit 실패 |
| q15 | `55167107eee0fa66a880a355b430cb45f5d89979` | source-qualified prefix compiler 연결; 관측 124개·직접 filter 7개 focused 및 release 완료; 8-lock source-filter ABBA 완료·동등성 확인, P7P4 정책 work-limit 실패와 multiset 0 대조의 freeze 후 SIGKILL; 11-lock 전체 worker 종료 후 coordinator merge/import node-limit 실패 |
| q16 | `86aa5b8e255c53636bfacc21a04c4efa706c2759` | exact multiset group index 연결; 관측 129개·release 완료; 8-lock ABBA 완료·동등성 확인·총시간 개선 미관측, P7P4 정책 work-limit 실패; 같은 producer의 관측 work 하계 때문에 11-lock 반복 실행은 하지 않음 |
| q17 | `7108df03b50df28c8a04eae7dd71f1e2f22dd76d` | exact incumbent branch-bound 연결; 관측 135개 focused·release 완료; 8-lock ABBA 완료·원본 coverage 동등성 확인·평균 비율 0.411130754380963; P7P4 group 0/1 모두 정책 work-limit 실패 |
| q18 | `3d460d4a2c1ae44193142ec1762ebd4e1d877378` | 완료 H cone 재사용; 관측 141개·release 및 8-lock ABBA 완료, 총시간 개선 미관측; P7P4 policy work-limit, 11-lock 모든 worker 완료 뒤 중앙 node-limit 및 producer work 71,396,217 |
| q19/q20 통합 | `13d539b`, 후속 class 정렬 개선 `d6df081` | 완료 H 집합 class와 실제 공개 공급 bitmap; 관측 161개 통합 검증, 실제 성능은 q21 binary로 계측 |
| q21 | `2d99de404e567d7e83eaed321d447bcc3e66a9cd` | 중간 T exact cone까지 확장; 관측 166개·release, 8-lock cone/class 별도 ABBA 완료; bitmap 1의 8-lock 실패 및 P7P4 class 0/1의 서로 다른 단계 work-limit |
| q22 | `8f63f0cdbd1323de6eb454f4e4bc9b08accdc25f` | typed 경계 remap·완료 unordered roots·cone 3·target-M/board bitmap; 관측 176개·release, 8-lock cone/bitmap 두 ABBA 완료·동등성 확인, P7P4는 정책 work-limit |
| q23 | `055427a8381a25e38646013b57eb150758b44b8e` | request 소유 공유 producer·worker별 증분 kind importer; 관측 179개·importer 5개 및 release 완료, 8-lock shared ABBA 완료·동등성 확인, P7P4 group 0/1 정책 work-limit; 11-lock 생성 중 8M suffix 참조 상한 |

같은 비교 표 안의 가지치기 arm은 같은 바이너리를 사용했다. microbenchmark와 전체 oracle은 서로 다른 소스·바이너리의 결과이므로 양쪽 시간을 더하거나 서로의 상대 속도로 해석하지 않는다.

| 보관 바이너리 또는 역할 | SHA256 |
| --- | --- |
| `bin/baseline-observation` | `1b48ff80b80c7ccc31dc93188c1c94eec78b20211aa4fd6684dd09628a378aed` |
| `bin/quotient1-observation` | `c29535e89f140092ea1e1a9c2487f42993fd1ec6308b9d41536e2eace64b6547` |
| `bin/quotient1-placement` | `02b96d6db871b126f99d3ed1fca10d337ed0586d735060e529b06d2cec35ea5d` |
| `bin/quotient2-observation` | `6e82efd9c83c9a835bfa1f92cd182609291b54362a62e81f04598d74c86bd872` |
| `bin/quotient3-observation` | `7e29c223a80eaf4b58b76cfc640c62b5ae85e62b0efb7962ca29b9b1e56e4733` |
| `bin/quotient4-observation` | `f9bdb5d675da8f50ee3cd679ab867c49efe79012b51cad1e19b7383d3b5808d7` |
| `bin/quotient5-observation` | `e58aea7f9b647ef6d065cef90c8541de859c9e4982d8aa6321cdd1dff99b1d14` |
| `bin/quotient6-observation` | `aac9ea5bdfd4367a4201731e2652b5a349597babf23044ef714bebdbe6659527` |
| `bin/quotient7-observation` | `30348d55483bef01f60dade5cfab321b659e2aa19c4fb1df3d8ccff019f4b52c` |
| `bin/quotient9-observation` | `abeb8f5a230813009592174b9a7ad72fb7096682a79e69d4ef582a10de91ebce` |
| `bin/quotient10-observation` | `fe27586f2adf2679c045665f328cf24cc94017b4f1c0f21c015aa821428d708d` |
| `bin/quotient11-observation` | `e14b42bcd2d7e291a09e890294d4c6723b90bf44278f7750e9bf119acacf0e4b` |
| `bin/quotient13b-observation` | `557010cb32ba7c70e3b966f8b923d546d2eec406f70572d8ea3db104ef326b5b` |
| `bin/quotient14-observation` | `998970d1f385fa319fb17ae40701d9837d633e60cfecbe234260da684f34a897` |
| `bin/quotient15-observation` | `7f2a1dd051fb04f8ad46d027ca46a3e8f6b97ccce58bd3006eadf71e6a79663e` |
| `bin/quotient16-observation` | `2ea39e9bbf51377ae162888bfe976768c7c34293382f7a595f0cd8f8e0aa6c13` |
| `bin/quotient17-observation` | `72ee31e173eb559e3c454bcae604f742812b88844ae676b6981a935f9ada0798` |
| `bin/quotient18-observation` | `9ae331d323e9216661b4933082fb039ac118ef747e02c43b9569ffc0835a9610` |
| `bin/quotient21-observation` | `f1578b9a749461c18ec5b27a256a708957ad48099ed0a56c75ff6895f76e5b3d` |
| `bin/quotient22-observation` | `9cf6702d030d27b463f780f05a7b67f4e4c6bdea62e26d517f80727519402197` |
| `bin/quotient23-observation` | `edb5a6f789143c087a79fd81eb05b2abc4668850a7009f36f8dd34f251ec73fd` |
| q2 microbenchmark test binary | `38b06b750387b03e81fc7aeb9eba5d403d6cf1766bb9c57ce8c1709627e7b110` |

일반 검색은 native Rust CPU 제품 경로, `WorkerPolicy::Auto`, `use_all_logical_processors=true`를 사용했다. 보고된 CPU hard/affinity/available limit와 실제 `workers_used`, `parallel_active_workers`는 모두 9다. tablebase·conditioned reachability·exact legal-board 가속은 요청에서 비활성화했다.

oracle 비교에서는 `CLEARRA_STRICT_PREFIX=1`, `CLEARRA_PRODUCT_PREFIX_ROUTE=1`을 고정했다. 후자는 실험용 strict producer를 적용하지 않고 일반 요청의 자동 dependency admission을 사용한다. `legacy_flag=false`, `eligible=true`, `strict_experiment_applied=false`가 결과에 기록되어 있다. 변경 축은 `CLEARRA_STRICT_GLOBAL_KICK_PRUNE=0/1/2`뿐이다.

## 3. see-7의 실패 단계와 실제 8-lock 완료 비교

| 실행 | 관측 계약·fixture | `execution_seconds` | 결과 |
| --- | --- | ---: | --- |
| baseline | `p7p4 visible-7`, 9 logical processors | 85.121212213 | `complete=false`, `wasm_observation_physical_work_limit` |
| q1 | `p7p4 visible-7`, Current, 9 logical processors | 92.701018051 | `complete=false`, `wasm_observation_physical_work_limit` |
| q2 | `p7p4 visible-7`, Current, 9 logical processors | 101.39507698 | `complete=false`, `wasm_observation_physical_work_limit` |

근거 artifact는 `baseline-p7p4-visible7.log`, `quotient1-p7p4-visible7.json`, `quotient1-p7p4-visible7.stderr`다. baseline 기록에는 구체적인 registration 단계 계측이 없으므로 q1의 위치 설명을 baseline에도 그대로 적용하지 않는다.

q1에는 실패 직전 registration 진단이 있다.

| q1 registration 항목 | 기록값 |
| --- | ---: |
| 등록된 candidate | 369,928 |
| 등록된 shared relation | 326,802 |
| suffix binding | 16,703,989 |
| 이미 청구한 work | 63,999,970 |
| 다음 요청의 attempted steps | 49 |
| 명시적 work 상한 | 64,000,000 |

따라서 q1에서 확인된 다음 병목은 정책 value 평가의 board memo가 아니라, 완료 관계의 등록·보유 과정이었다. 공개 후 Frontier 표현을 적용해도 registration 상한을 먼저 넘었다. q2는 이 지점의 relation 공유와 필요 없는 suffix binding 생략을 대상으로 했다. 상한 오류를 낮은 성공확률이나 정상 완료 0으로 바꾸지 않았다.

### 3.1 q2: 등록·merge를 마친 뒤 초기 정책 확장에서 거절

`quotient2-p7p4-visible7.json`과 `quotient2-p7p4-visible7.stderr`는 전체 456,459 candidate factor를 가진 최종 factored view에서 평가를 시작한 뒤 실패했음을 기록한다.

| q2 실패 시 최종 view 항목 | 기록값 |
| --- | ---: |
| candidate factors | 456,459 |
| proof factors | 358,386 |
| suffix bindings | 13,685,796 |
| physical nodes / edges | 18 / 17 |
| residual members | 865,448 |
| physical source visits | 456,459 |
| kind source visits / revealed source visits | 0 / 0 |
| revealed nodes / frontier nodes | 0 / 0 |
| 마지막으로 청구에 성공한 누적 work | 63,650,662 |

q2에서는 registration/merge를 끝내고 실제 정책 확장으로 진입했다. 그러나 이전 단계에서 청구한 work가 같은 owner의 평가 단계로 이어져, 초기 physical derivative 확장 중 64,000,000 한도에 도달했다. 기록된 work는 마지막 성공한 청구값이며, 거절된 다음 청구를 포함한 완료 작업량이 아니다.

q1의 부분 등록 진단과 q2의 전체 merge 후 진단은 모집단과 단계가 다르다. 이 숫자를 빼서 같은 범위의 work 절감률로 표현하지 않는다. q2가 더 멀리 진행했다는 사실은 확인했지만 full see-7의 완료나 end-to-end 속도 개선은 확인하지 못했다. q3의 source-relative whole-PC 물리 board DAG closure 결과는 §3.2에서 별도 fixture로 기록하며, 이 q2 측정에 적용된 것으로 취급하지 않는다.

세 실패시간은 서로 다른 바이너리가 각자의 resource 경계에 도달할 때까지 걸린 시간이다. 어느 쪽이 더 빠른 정확한 solver인지, 전체 see-7이 개선되었는지를 뜻하지 않는다.

### 3.2 q3: 실제 8-lock은 두 경로 모두 완료, board 경로가 더 느림

근거는 `quotient3-eight-factored.json`, `quotient3-eight-board.json`, `quotient3-eight-comparison.json`과 각 stderr다. 같은 q3 소스·바이너리, 4L 초기 보드 `0xff`, 8 lock, 9 draw, SRS+ 규칙, VisibleSeven, Auto/9-worker를 사용했다. `CLEARRA_STRICT_GLOBAL_KICK_PRUNE=1`을 명시해 가지치기를 Current로 고정하고, `CLEARRA_OBSERVATION_BOARD_QUOTIENT`만 0/1로 바꿨다. 현재 제품 기본 Improved(2)의 효과를 이 비교에 섞지 않았다.

독립 Rust 비교기는 `validation_passed=true`, `equivalent=true`, `contract_differences=[]`, `result_differences=[]`를 반환했다. 두 경로 모두 probability·packing·objective 완료, resource truncation 없음, BuildUp node 0 조건을 통과했다. 이는 미완료 실패시간이 아니라 완료 결과끼리의 비교다.

| 완료 결과 | 두 경로의 공통 기록값 |
| --- | --- |
| 원래 공급 pattern 수 | 211,680 |
| 선택된 see-7 정책이 성공하는 원래 pattern 수 | 211,234 |
| coverage probability | `0.9978930461073318` |
| coverage semantics | `visible-seven-policy` |
| 전체 원본 coverage words SHA256 | `24d3271c4b4af592b97186ce2a0b542c032b4b98c7e93fcb8774a9a687ba9797` |
| packing candidate count | 27,987 |
| 실제 worker / active worker | 9 / 9 |
| BuildUp node | 0 |

성공확률이 1보다 작다는 것은 미완료를 뜻하지 않는다. 이 결과는 원래 211,680개 공급을 모두 평가한 완료된 VisibleSeven 정책의 성공 집합이다. 두 arm에서 원래 ordinal의 성공 부분집합을 가리키는 전체 coverage SHA 일치까지 확인했다. 이 coverage-summary 출력은 solution identity와 candidate-set digest를 계산하지 않는다. 따라서 q3에는 §5.3 oracle에서 확보한 full materialized identity SHA를 옮겨 붙이지 않으며, 같은 candidate count만으로 candidate 집합의 equality를 실측했다고 주장하지 않는다.

| 측정 항목 | factored, board mode 0 | board closure, board mode 1 |
| --- | ---: | ---: |
| 전체 `execution_seconds`, 각 1회 | 204.15366027 | 231.933298904 |
| 정책 memo 상태 | 161,008,165 | 189,236,250 |
| 정책 action 검사 | 173,974,653 | 195,592,893 |
| 관측 trie node | 260,660 | 260,660 |
| producer 등록 candidate | 2,597 | 2,597 |
| producer work 카운터 | 268,039 | 176,660 |
| worker 관측 producer retained bytes | 11,258,340 | 10,462,720 |

q3 board 표현에는 2,130개 board node, 4,045개 edge, 34,600개 suffix root가 남았다. prefix 방문은 35,949회, edge 시도는 48,868회다. factored 표현에는 physical node 104개·edge 3,651개 외에 revealed node 2,046개와 frontier node 1,967개, residual member 64,277개, suffix binding 35,491개가 있다. 두 표현의 node와 work 카운터는 서로 다른 작업 단위를 세므로 일부 숫자만으로 전체 검색비용의 감소율을 계산하지 않는다.

board stderr의 단계 진단은 다음과 같다.

| board 평가 단계 | 초 |
| --- | ---: |
| 원본 공급으로 관측 evaluator 구성 | 0.0397643 |
| board view freeze | 0.000665138 |
| 정책 평가 | 231.467656458 |

여기서 `source_seconds`는 최종 관측 evaluator의 원본 공급 준비 단계이며, 앞선 전체 geometry 생산 시간을 뜻하지 않는다. 이 단계 시간은 전체 `execution_seconds`에 포함되므로 다시 더하지 않는다. 측정된 주 비용은 정책 평가에 있고, board producer의 작은 표현과 낮은 producer work가 정책 상태 증가를 상쇄하지 못했다. 각각 1회 측정으로 안정적인 회귀율을 추정하지는 않지만, 이 실행에서 board closure가 end-to-end 개선이었다고 결론 내릴 수는 없다.

### 3.3 q4: 같은 바이너리에서 revealed kind union의 효과 확인

근거는 `quotient4-eight-alternatives.json`, `quotient4-eight-union.json`, `quotient4-eight-comparison.json`과 각 stderr다. q3와 같은 실제 8-lock/9-draw fixture를 사용하지만, 시간 비교는 q4의 같은 소스·바이너리끼리만 수행했다. board mode 1, Current kick pruning 1, 9-worker를 고정하고 `CLEARRA_OBSERVATION_KIND_UNION`만 0/1로 바꿨다. q3의 이전 바이너리를 q4의 대조군으로 사용하지 않는다.

Rust comparator는 `validation_passed=true`, `equivalent=true`, `changed_axes=["kind_union_mode"]`, 빈 contract/result differences를 보고했다. 두 실행 모두 전체 211,680개 원래 공급 중 211,234개 성공, 확률 `0.9978930461073318`, 전체 coverage words SHA256 `24d3271c4b4af592b97186ce2a0b542c032b4b98c7e93fcb8774a9a687ba9797`로 일치했다. probability·packing·objective 완료, BuildUp node 0, resource truncation 없음도 유지했다. candidate-set digest와 full materialized solution identity를 계산하지 않는 coverage-summary의 증거 범위는 §3.2와 같다.

| 측정 항목 | kind alternatives, mode 0 | revealed kind union, mode 1 |
| --- | ---: | ---: |
| 전체 `execution_seconds`, 각 1회 | 233.053567468 | 43.714876175 |
| 정책 평가 단계, 초 | 232.574416976 | 43.327611276 |
| 정책 memo 상태 | 187,568,639 | 35,087,187 |
| 정책 action 검사 | 193,640,955 | 56,339,679 |
| 관측 trie node | 260,660 | 260,660 |
| board node / edge | 2,130 / 4,045 | 2,130 / 4,045 |
| board suffix root | 32,043 | 32,043 |
| kind union node / edge | 0 / 0 | 61,753 / 165,960 |
| kind union member | 0 | 762,504 |
| kind union source visit | 0 | 2,436,684 |
| kind union work | 0 | 3,857,640 |
| worker 관측 producer retained bytes | 10,158,028 | 10,331,144 |

q4는 완전히 공개된 이후의 kind alternative들을 union 관계로 재사용해 같은 suffix에 대한 별도 정책 평가를 줄인다. 이 실행에서는 board 크기와 suffix root 수를 유지한 채 정책 상태·action 검사가 줄었고, 총시간 감소도 정책 평가 단계에서 나타났다. union 구축·보유 비용은 위 별도 계측에 포함되며 0으로 취급하지 않는다. producer retained bytes는 전체 정책 memo 또는 RSS peak를 뜻하지 않는다. 각 arm은 1회이므로 반복성이나 안정적인 개선율을 추정하지 않는다.

세 축을 표시하는 새 Rust 비교기로 같은 두 입력을 다시 검증한 `quotient4-eight-comparison-v3.json`도 `validation_passed=true`, `equivalent=true`다. 이 출력의 단일 축 0→1 시간 평균 비율은 `0.187574370347291`이며, board 1/source `legacy`를 고정한 kind union 두 sample의 기술 통계다. 각 arm의 표본수는 여전히 1이다.

### 3.4 q4 전체 P7P4: 정책 평가 전에 suffix root 상한으로 거절

`quotient4-p7p4-union.json`의 `execution_seconds=88.292204304`는 완료 시간이 아니다. 실행은 `complete=false`, `InvalidProblem { reason: "wasm_observation_physical_union_limit" }`로 끝났으며, stderr의 `clearra_observation_board_suffix_limit`가 board 관계 등록 단계의 구체적인 원인을 보여준다.

| 실패 시 board producer 진단 | 기록값 |
| --- | ---: |
| 그 시점까지 등록한 candidate | 253,966 |
| board node / edge | 688,308 / 2,127,017 |
| suffix root | 8,000,000 |
| prefix visit | 13,494,008 |
| edge attempt | 29,571,418 |
| 마지막 계측 work | 62,583,912 |
| 진단의 retained bytes | 217,365,316 |

이 진단은 전체 family가 준비되기 전의 부분 등록 상태다. 같은 데이터의 완료 결과나 정책 평가량으로 사용하지 않는다. 8-lock에서 공개 후 정책 평가를 줄인 q4도 전체 P7P4에서는 앞선 suffix 보유 상한을 통과하지 못했다. 따라서 88.292204304초를 q1/q2의 미완료 시간과 비교해 speedup으로 보고하지 않으며, P7P4의 see-7 성공확률·전체 coverage SHA도 없다.

### 3.5 q5: P7P4는 merge work 사전 청구에서 거절

- [x] 수정 파일: board plan admission과 관계 producer, 구현 내용: 실제 fresh·균일 공급에서 첫 bag 범위 안에 머무는 hidden prefix에 kind별 공급 상한을 부여했다. 초기 held piece는 해당 kind의 추가 1개로 구분하며, 첫 bag 경계 밖에는 그 상한을 발급하지 않는다. source·Hold·관측 경계에 근거한 가지치기이며 resource limit를 높인 변경이 아니다.
- [x] 수정 파일: 관측 관련 구현과 경계 테스트, 구현 내용: q5의 `quotient5-observation-tests-fixed.log`에서 `78 passed; 0 failed`를 확인했다. `quotient5-release-build.log`에는 release build 완료가 기록되어 있다. 테스트 통과는 실제 큰 검색의 완료나 개선율을 뜻하지 않는다.
- [x] 수정 파일: 이 문서, 구현 내용: q5의 실제 P7P4 source prefix mode 1을 실행하고 `wasm_observation_physical_work_limit` 종료를 확인했다. 정책 평가의 완료 결과는 없다.
- [x] 수정 파일: 이 문서 및 관측 benchmark artifact, 구현 내용: q5의 실제 8-lock에서 source prefix 0/1의 완료 결과를 비교했다. 결과와 비용의 범위는 §3.6에 기록했다.
- [ ] 수정 파일: 이 문서 및 후속 관측 benchmark artifact, 구현 내용: 전체 P7P4의 10-lock과 실제 11-lock/12-draw를 완주한다. 현재 두 범위의 see-7 완료 확률·완주시간은 없다.

근거는 `quotient5-p7p4-union.json`과 stderr다. board 1/kind union 1/source prefix 1, kick pruning 1, 9-worker에서 **97.094348033초 후 미완료 오류**로 끝났다. `clearra_observation_board_work_limit` 진단은 다음과 같다.

| 실패 시 board producer 진단 | 기록값 |
| --- | ---: |
| 그 시점까지 등록한 candidate | 352,930 |
| board node / edge | 474,634 / 1,464,999 |
| suffix root | 7,297,815 |
| prefix visit | 17,991,878 |
| edge attempt | 27,946,275 |
| 이미 청구한 work | 57,422,082 |
| incoming shard를 받기 전 다음 work 청구 | 7,888,073 |
| source 조건으로 가지치기한 prefix | 4,431,529 |
| dead source prefix | 54,314 |
| 진단의 retained bytes / peak bytes | 151,334,448 / 198,535,192 |

`merge_inner`는 incoming shard의 `source.metrics.work + 1`을 먼저 청구한다. 이 청구를 기존 work에 더하면 64,000,000 상한을 넘으므로, 다음 shard의 실제 merge 전에 거절되었다. 표의 누적 work와 구조 카운터는 거절된 merge가 수행되기 전의 owner 상태다. source 가지치기가 실제로 적용되었다는 계측은 있지만, 전체 family 등록·정책 평가를 마쳤다는 뜻은 아니다. q4와 q5는 실패 시점의 등록 모집단도 다르므로 두 부분 진단의 node·memory 차이를 같은 완료 workload의 절감률로 보고하지 않는다. 97.094348033초도 실패 종료시간이며 완료 확률·전체 coverage SHA·speedup 근거가 아니다.

관측 JSON v4는 `board_quotient_mode`, `kind_union_mode`, `source_prefix_mode`를 각각 기록한다. 같은 board/kind 조합에서 source prefix만 0/1로 바꾼 비교로 새 가지치기의 효과를 분리한다. v2에는 kind 축이 없고 v2/v3에는 source prefix 축이 없으므로 비교기는 해당 축을 `legacy`로 표시한다. 서로 다른 schema·source·binary를 하나의 유효한 비교 묶음으로 인정하지 않는다. v4의 새 `observation_board_source_pruned_prefixes`, `observation_board_dead_source_prefixes`는 정수 계측으로 검증하며 의미 결과의 동등성 검사와 분리한다.

### 3.6 q5: 8-lock source prefix 0/1의 결과 동등성 확인, 총시간 개선은 관측하지 못함

근거는 `quotient5-eight-source-off.json`, `quotient5-eight-source-on.json`, `quotient5-eight-comparison.json`과 각 stderr다. 같은 q5 소스·바이너리, board 1/kind union 1/kick pruning 1, 9-worker를 고정하고 source prefix만 0/1로 바꿨다. 비교기의 `validation_passed=true`, `equivalent=true`, `changed_axes=["source_prefix_mode"]`, 빈 contract/result differences를 확인했다.

두 실행 모두 전체 211,680개 원래 공급 중 211,234개 성공, 확률 `0.9978930461073318`, coverage words SHA256 `24d3271c4b4af592b97186ce2a0b542c032b4b98c7e93fcb8774a9a687ba9797`로 일치한다. probability·packing·objective 완료와 BuildUp node 0도 유지했다. packing candidate count는 27,987로 같으며, candidate digest와 full materialized identity가 미계산이라는 한계는 그대로다.

| 측정 항목 | source prefix 0 | source prefix 1 |
| --- | ---: | ---: |
| 전체 `execution_seconds`, 각 1회 | 42.104770599 | 42.220748636 |
| 정책 평가 단계, 초 | 41.778400573 | 41.80205918 |
| 정책 memo 상태 | 35,087,099 | 35,057,867 |
| 정책 action 검사 | 56,339,397 | 56,290,248 |
| board node / edge | 2,130 / 4,045 | 2,018 / 3,790 |
| board suffix root | 32,043 | 31,269 |
| board prefix visit | 35,949 | 35,949 |
| board edge attempt | 48,868 | 47,550 |
| board work | 171,357 | 168,176 |
| source-pruned / dead source prefix | 0 / 0 | 866 / 0 |
| kind union node / edge | 61,753 / 165,960 | 61,653 / 165,783 |
| kind union member | 762,504 | 760,890 |
| kind union source visit | 2,436,684 | 2,431,516 |
| kind union work | 3,857,640 | 3,850,355 |
| worker 관측 producer retained bytes | 9,995,488 | 10,424,316 |

Rust comparator의 source 1/source 0 시간 평균 비율은 `1.0027545106017692`다. 첫 bag에서 불가능한 prefix 866개를 제거하면서 일부 구조·정책 카운터는 줄었지만, 이 완료 pair에서는 전체 검색시간을 줄이지 못했다. 각각 1회이므로 작은 시간 차이를 안정적인 성능 회귀로 추정하지도 않는다. producer retained bytes는 오히려 더 컸으므로 node 감소만으로 메모리 개선을 주장하지 않는다. 이 계측은 worker 보유 allocation을 포함하며 전체 실행의 RSS peak를 뜻하지 않는다.

### 3.7 q5의 11-lock 제한 probe는 timeout

q5의 실제 11-lock/12-draw probe는 180초 timeout을 설정한 감독 실행에서 시간 상한으로 종료되었다. `quotient5-eleven-probe.stderr`에는 `E_CLEARRA_PROCESS_TIMEOUT`과 `reason=timeout`, 감독 receipt ID `1795325923231365760-5`가 기록되어 있다. `quotient5-eleven-probe.json`은 비어 있으므로 완료 확률·전체 coverage SHA·완료 단계 계측은 없다. 이 제한 probe는 11-lock 완주도 정상적인 실패 확률 0도 아니다. 내부 어느 단계가 시간 상한에 도달했는지는 이 출력만으로 특정하지 않는다.

### 3.8 q6: 8-lock 경계 suffix 비교 완료, 한 pair에서 총시간 감소

q6는 관측 경계 H의 suffix binding만 보유하는 변경을 적용했다. `quotient6-observation-tests-fixed.log`에서 관측 테스트 80개 통과를 확인했고, 실제 비용은 `quotient6-eight-boundary-off.json`, `quotient6-eight-boundary-on.json`, `quotient6-eight-comparison.json`과 각 stderr로 확인했다. 같은 q6 소스·바이너리에서 board/kind/source를 모두 1, kick pruning을 1, worker를 9로 고정하고 boundary suffix만 0/1로 바꿨다. 실행 순서는 boundary 1 뒤 boundary 0이며 각 1회다.

Rust comparator는 `validation_passed=true`, `equivalent=true`, `changed_axes=["boundary_suffix_mode"]`, 빈 contract/result differences를 보고했다. 두 arm의 원래 공급 211,680개 중 211,234개 성공, 확률 `0.9978930461073318`, 전체 coverage words SHA256 `24d3271c4b4af592b97186ce2a0b542c032b4b98c7e93fcb8774a9a687ba9797`가 같다. probability·packing·objective 완료, BuildUp node 0도 유지했다. candidate digest와 full materialized identity 미계산의 증거 범위를 확대하지 않는다.

| 측정 항목 | boundary suffix 0 | boundary suffix 1 |
| --- | ---: | ---: |
| 전체 `execution_seconds`, 각 1회 | 43.755515787 | 40.142549954 |
| 정책 평가 단계, 초 | 43.369215012 | 39.756690785 |
| 정책 memo 상태 | 35,058,041 | 33,294,604 |
| 정책 action 검사 | 56,290,684 | 55,993,154 |
| board node / edge | 2,018 / 3,790 | 2,018 / 3,790 |
| board suffix root | 31,269 | 21,158 |
| board prefix visit | 35,949 | 35,949 |
| board edge attempt | 47,550 | 47,550 |
| board work | 167,890 | 147,650 |
| source-pruned prefix | 866 | 866 |
| kind union node / edge | 61,653 / 165,783 | 39,868 / 105,178 |
| kind union member | 760,890 | 314,671 |
| kind union source visit | 2,431,516 | 1,065,198 |
| kind union work | 3,850,355 | 1,736,625 |
| worker 관측 producer retained bytes | 10,314,696 | 10,237,340 |

Rust가 계산한 boundary 1/boundary 0 시간 평균 비율은 `0.9174283340507797`다. 이 실행에서는 물리 board node·edge를 유지하면서 suffix 보유와 downstream kind union 작업이 줄었고, 정책 평가 및 전체 시간도 낮았다. 한 pair의 기술 통계이며 반복성이 검증된 개선율은 아니다. 별도 바이너리인 q5의 시간을 대조군에 섞지 않는다.

### 3.9 q6 전체 P7P4: suffix 축소 후에도 work 상한에서 미완료

`quotient6-p7p4-union.json`은 board/kind/source/boundary 모두 1, kick pruning 1, 9-worker에서 **92.704597752초 후 `wasm_observation_physical_work_limit`**로 종료되었다. `quotient6-p7p4-union.stderr`의 마지막 진단은 다음과 같다.

| 실패 시 board producer 진단 | 기록값 |
| --- | ---: |
| 그 시점까지 등록한 candidate | 411,520 |
| board node / edge | 534,090 / 1,670,421 |
| suffix root | 3,964,085 |
| prefix visit | 21,433,725 |
| edge attempt | 33,478,670 |
| 이미 청구한 work | 63,711,712 |
| 다음 work 청구 | 6,062,705 |
| source-pruned / dead source prefix | 5,316,599 / 63,291 |
| 진단의 retained bytes / peak bytes | 178,877,916 / 260,886,924 |

기존 work에 다음 청구를 더하면 64,000,000 상한을 넘는다. 따라서 이 수치는 전체 family·정책 평가의 완료 계측이 아니며, 거절된 다음 작업을 실행한 값도 아니다. suffix 보유를 줄인 q6도 전체 P7P4의 완료 확률이나 coverage SHA를 얻지 못했다. q5와 다른 부분 등록 모집단을 비교해 완료 workload의 메모리·작업 절감률을 계산하지 않으며, 92.704597752초를 실패 종료시간 speedup으로 사용하지 않는다.

### 3.10 q7: 전체 factor 등록을 마친 뒤 freeze의 edge 재구성에서 거절

q7은 candidate별 물리 edge 삽입을 생략한 뒤 freeze에서 보유 board 사이의 합법 edge를 같은 workspace의 규칙·도달성 권한으로 재구성한다. `quotient7-p7p4-edge-on.json`과 stderr는 board/kind/source/boundary/edge 모두 1, kick pruning 1, 9-worker에서 **96.746755932초 후 `wasm_observation_physical_work_limit`**로 종료된 것을 기록한다. edge mode의 제품 기본은 0으로 유지했다.

이번에는 456,459개 candidate factor의 등록·merge를 마친 뒤 freeze의 edge 재구성으로 진입했다. 이 단계 진전은 q6의 부분 등록 실패와 구분된다. 그러나 실제 정책 평가의 완료 확률이나 전체 coverage SHA는 아직 없다. 아래 work는 상한 시점의 누적값이며 종료시간은 전체 실행의 실패 시간이다. freeze만의 소요시간은 이 로그에 따로 기록되지 않았다.

| 실패 시 board producer 진단 | 기록값 |
| --- | ---: |
| 등록 candidate factor | 456,459 |
| board node / 생성된 edge | 569,195 / 789,698 |
| suffix root | 4,361,665 |
| prefix visit | 23,789,092 |
| edge attempt | 789,698 |
| 이미 청구한 work / 다음 청구 | 64,000,000 / 1 |
| source-pruned / dead source prefix | 5,913,906 / 69,615 |
| 진단의 retained bytes / peak bytes | 161,895,456 / 216,687,464 |

| freeze closure 계측 | 기록값 |
| --- | ---: |
| pose 검사 | 14,702,013 |
| harddrop fast-path hit | 708,303 |
| exact reachability query | 74,743 |
| visited reachability state | 14,890,884 |
| 처리한 parent board | 36,664 |

| 원래 lock 깊이 | 보유 node | 그 시점까지 생성한 outgoing edge | suffix root |
| --- | ---: | ---: | ---: |
| 0 | 1 | 162 | 0 |
| 1 | 162 | 12,473 | 0 |
| 2 | 8,361 | 193,798 | 0 |
| 3 | 121,497 | 583,265 | 0 |
| 4 | 439,174 | 0 | 4,361,665 |

전체 factor의 등록은 끝났지만 이 edge 목록은 freeze가 중단된 부분 결과다. q7은 candidate별 반복 삽입 대신 parent board별 합법 edge를 만드는 다른 작업 구성을 사용하므로, 이전 `edge_attempts`와 단순 비율로 비교해 총비용 개선을 선언하지 않는다. 확인된 추가 비용은 pose·harddrop 검사와 exact reachability 방문이며, 그 비용까지 포함해 상한에 도달했다. q7의 96.746755932초도 완료된 solver의 속도 비교값이 아니다.

### 3.11 q9: 전체 등록·freeze 완료 후 정책 kind union에서 거절

q9는 마지막 숨은 층의 typed edge를 유지하고 그보다 앞선 층의 legal edge만 재구성한다. `quotient9-p7p4-edge-hybrid.json`과 stderr는 같은 9-worker·kick mode 1·board/kind/source/boundary mode 1, edge mode 2, collection mode 1에서 **107.122856232초 후 `wasm_observation_physical_work_limit`**를 기록한다. q8의 전체 성공 subtree 수집 단축도 이 바이너리에 포함되어 있다.

stderr의 `board_ready`는 전체 456,459 candidate factor의 등록·merge와 freeze를 마쳤음을 보여 준다. 이후 정책 평가 중 kind union에서 다음 work 1을 청구하다 거절되었다. q7의 freeze 내부 실패와 달리 실제 정책 평가에 진입했지만, 최종 coverage SHA·확률·completeness 결과는 없다.

| q9의 frozen board producer | 기록값 |
| --- | ---: |
| 등록 candidate factor | 456,459 |
| board node / edge | 569,195 / 1,803,660 |
| suffix root | 4,361,665 |
| prefix visit / edge attempt | 23,789,092 / 15,714,669 |
| producer base work | 62,477,802 |
| source-pruned / dead source prefix | 5,913,906 / 69,615 |

| 정책 평가에서 거절될 때의 kind union | 기록값 |
| --- | ---: |
| node / edge | 30,376 / 78,190 |
| member / source visit | 283,015 / 978,497 |
| kind union work / 다음 청구 | 1,522,198 / 1 |
| kind union retained bytes / peak bytes | 4,997,244 / 5,353,588 |

| q9의 별도 단계 계측 | 초 |
| --- | ---: |
| observation source 준비 | 1.490313838 |
| board freeze | 0.399406499 |
| 정책 평가, 거절까지 | 7.475582441 |
| 전체 실행, 거절까지 | 107.122856232 |

source 준비 시간은 observation evaluator를 준비하는 구간이며 Geometry 전체 시간이 아니다. 위 kind union byte 값도 전체 process peak가 아니다. producer가 보유한 work와 정책 union work가 같은 64M 상한을 공유하므로, 등록·freeze를 통과했다는 사실만으로 충분한 정책 예산이 남는 것은 아니다. q7·q9의 실패시간을 비교해 speedup을 만들지 않는다.

### 3.12 q8/q9: 같은 바이너리 8-lock 네 arm의 완료 비교

`quotient9-eight-edge0-collect0.json`, `quotient9-eight-edge1-collect0.json`, `quotient9-eight-edge2-collect0.json`, `quotient9-eight-edge2-collect1.json`을 아래 순서로 실행했다. 같은 source·binary, board/kind/source/boundary mode 1, kick mode 1, Auto/9-worker를 유지했다. 기존 v7 입력 지원 Rust 비교기의 `quotient9-eight-comparison.json`은 비교 출력 v6이며 `validation_passed=true`, `equivalent=true`다.

네 arm 모두 **211,234 / 211,680개의 원래 ID**, 확률 `0.9978930461073318`, 전체 coverage SHA256 `24d3271c4b4af592b97186ce2a0b542c032b4b98c7e93fcb8774a9a687ba9797`이 같다. 확률·공급·packing·objective의 완료 gate와 `BuildUp=0`도 유지한다. 이 CoverageSummary 실행은 전체 solution identity 또는 candidate 집합 hash를 계산한 실행이 아니다.

| 실행 순서 | `(edge, collect)` | 전체 초 | observation source 준비 초 | freeze 초 | policy 초 |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 | `(0,0)` | 43.442057414 | 0.024026459 | 0.000538773 | 43.093624588 |
| 2 | `(1,0)` | 40.934670986 | 0.026145397 | 0.006213727 | 40.623229811 |
| 3 | `(2,0)` | 38.323224955 | 0.025445867 | 0.000629891 | 38.034574596 |
| 4 | `(2,1)` | 37.41591991 | 0.02400174 | 0.000583941 | 37.138099894 |

| 완료 단일 축 비교 | Rust comparator의 treatment/reference 평균 비율 | arm별 표본수 |
| --- | ---: | ---: |
| collect 0, edge 0→1 | 0.9422820516048592 | 1 / 1 |
| collect 0, edge 0→2 | 0.8821687377690736 | 1 / 1 |
| collect 0, edge 1→2 | 0.936204543285736 | 1 / 1 |
| edge 2, collect 0→1 | 0.9763249296982345 | 1 / 1 |

이 실행 순서는 ABBA가 아니며 각 arm은 한 번이다. 특히 collect 0의 edge 0→2 전체 시간 차이를 closure 단독의 안정적인 개선율로 단정할 수 없다. freeze는 밀리초 이하 또는 수 밀리초인 반면 전체 시간의 대부분은 policy에서 소비한다. edge 1은 새 합법 edge를 더해 policy 작업도 바꾸므로, edge 재구성 비용과 downstream 작업을 함께 기록한다.

| 구조 계측 | `(0,0)` | `(1,0)` | `(2,0)` | `(2,1)` |
| --- | ---: | ---: | ---: | ---: |
| board node | 2,018 | 2,018 | 2,018 | 2,018 |
| board edge | 3,790 | 3,840 | 3,790 | 3,790 |
| board suffix root | 21,158 | 21,158 | 21,158 | 21,158 |
| board work | 147,660 | 177,806 | 138,091 | 137,991 |
| closure parent board / pose 검사 | 0 / 0 | 104 / 41,704 | 1 / 401 | 1 / 401 |
| closure harddrop hit | 0 | 3,698 | 103 | 103 |
| closure reach query / visited state | 0 / 0 | 144 / 29,907 | 0 / 0 | 0 / 0 |
| policy state | 33,294,607 | 33,443,261 | 33,294,533 | 33,294,527 |
| policy action check | 55,993,076 | 56,168,567 | 55,992,939 | 55,993,022 |
| kind union work | 1,736,625 | 1,737,754 | 1,736,625 | 1,736,625 |
| collection call | 1,560,890 | 1,560,890 | 1,560,890 | 11,517 |
| full-subtree collection | 0 | 0 | 0 | 10,634 |

q8에서는 수집 호출 감소가 직접 확인되지만 정책 value를 구하는 상태 수 자체가 크게 줄었다고 볼 수 없다. q9 hybrid는 이 fixture에서 처음 한 parent만 재구성하며, 전체 재구성의 exact reachability 호출을 피한다. 이것을 더 큰 P7P4의 정책 kind union 상한까지 해결했다는 증거로 사용하지 않는다.

| 메모리 계측, bytes | `(0,0)` | `(1,0)` | `(2,0)` | `(2,1)` |
| --- | ---: | ---: | ---: | ---: |
| worker observation producer retained | 10,261,572 | 10,160,320 | 10,237,748 | 10,234,092 |
| 결과의 `resource_peak_cpu_bytes` | 740,359,158 | 741,897,718 | 740,363,858 | 740,345,714 |
| 감독 receipt의 sampled process-group RSS peak | 1,031,602,176 | 957,657,088 | 1,101,492,224 | 973,819,904 |

stderr가 가리키는 runtime receipt는 실행 순서대로 `1795329000603879192-5`, `1795329044075193003-18`, `1795329085031876285-31`, `1795329123408362262-44`다. RSS의 source는 모두 `linux-sampled-process-group-rss`이며 `kernel_memory_peak`는 null이다. 계산된 owner peak와 sample 기반 process RSS를 서로 대체하지 않는다. edge 2/collect 0의 sampled peak가 edge 0/collect 0보다 크므로 전체 메모리의 일괄 감소도 주장하지 않는다. edge의 제품 기본은 0, `CLEARRA_OBSERVATION_COLLECT_FULL`의 기본은 1이다.

### 3.13 q9의 11-lock: 원인·내부 단계가 확인되지 않은 SIGKILL

`quotient9-eleven-edge-hybrid.json`은 0 bytes이고 stderr에는 runtime receipt 경로만 있다. receipt `1795329207758780571-5`는 **419,282 ms 후 SIGKILL 9 / exit 137**을 기록한다. source 준비·freeze·policy에 관한 내부 로그가 없으므로 마지막으로 진행한 단계를 추정하지 않는다.

| 감독 관측 | 기록값 |
| --- | --- |
| sampled process-group RSS peak | 7,663,796,224 bytes |
| soft / hard limit | 8,923,100,775 / 9,914,556,416 bytes |
| memory-pressure event | 0 |
| supervisor termination reason | null |
| kernel memory peak / OOM-kill delta | null / null |

이 증거는 600초 timeout이나 확인된 OOM 종료가 아니다. sampled RSS가 기록된 한도 아래였다는 사실만으로 외부 memory pressure를 배제할 수도 없고, SIGKILL만으로 OOM을 확정할 수도 없다. 빈 결과를 정상적인 확률 0이나 완료 요약으로 처리하지 않는다. 실패 종료시간은 성능 비교에 넣지 않는다.

### 3.14 q10 P7P4: 공급 개수 필터 작동 후에도 공통 work 상한으로 미완료

`quotient10-p7p4-multiset-on.json`과 stderr는 source `d17c26e`, schema v8, supply multiset 1, edge 2, collection 1, board/kind/source/boundary 1, kick 1, Auto/9-worker에서 **93.529510444초 후 `wasm_observation_physical_work_limit`**를 기록한다. 전체 후보 등록과 freeze는 완료했고 정책 평가에서 새 필터를 실행했지만 전체 coverage·확률 결과는 없다.

| q10의 frozen board producer | 기록값 |
| --- | ---: |
| candidate factor / board node | 456,459 / 569,195 |
| board edge / suffix root | 1,803,660 / 4,361,665 |
| prefix visit / edge attempt | 23,789,092 / 15,714,669 |
| base work | 62,449,043 |
| source-pruned / dead source prefix | 5,913,906 / 69,615 |
| 기존 kind arena node | 16,373,232 |
| closure pose 검사 / harddrop hit | 3,418,124 / 326,986 |
| closure reach query / visited state / parent board | 22,619 / 4,853,739 / 8,524 |

| 정책 거절 시 kind union·공급 필터 | 기록값 |
| --- | ---: |
| union node / edge | 16,573 / 24,577 |
| union member / source visit | 67,499 / 126,853 |
| union work / 다음 청구 | 1,550,957 / 1 |
| checked / pruned root | 414,839 / 390,547 |
| certificate node / edge | 56,383 / 150,667 |
| 공급별 alias | 27,665 |
| union retained / peak bytes | 4,056,236 / 4,965,180 |

| 별도 단계 시간 | 초 |
| --- | ---: |
| observation source 준비 | 1.2885592319999999 |
| freeze | 0.44073412 |
| 정책 평가, 거절까지 | 0.348828285 |
| 전체 실행, 거절까지 | 93.529510444 |

공급 count에 맞지 않는 root를 실제로 제외했지만 certificate 구성·조회와 alias lookup도 기존 work 한도에 포함된다. 이 단계까지의 짧은 policy 시간은 전체 정책 계산이 빨라졌다는 뜻이 아니다. q9와 q10은 종료 전에 처리한 정책 작업이 다르므로 실패시간·부분 node·부분 retained bytes로 완료 solver의 절감률을 만들지 않는다. root 개수 필터의 동치는 [증명 문서 B11](see7-quotient-pruning-proof-2026-11-22.md)을 따르며, 이 실행은 그 필터가 공통 상한 내 전체 P7P4를 끝냈다는 증거가 아니다.

runtime receipt `1795329995908869858-5`의 sampled process-group RSS peak는 **5,651,431,424 bytes**이며 kernel peak는 null이다. 이는 이 미완료 실행에서 관측한 값이다. stderr의 union peak와 전체 process RSS를 서로 대신 쓰지 않는다. q10의 같은 바이너리 8-lock supply multiset 0/1 완료 비교는 다음 절에 별도로 기록한다.

### 3.15 q10 실제 8-lock: supply multiset 0/1 ABBA 완료 비교

`quotient10-eight-multiset0a.json`, `quotient10-eight-multiset1a.json`, `quotient10-eight-multiset1b.json`, `quotient10-eight-multiset0b.json`의 순서로 네 실행을 마쳤다. 모두 source `d17c26e`, binary SHA256 `fe27586f2adf2679c045665f328cf24cc94017b4f1c0f21c015aa821428d708d`, schema v8, `eight / visible-7`, Auto/9-worker다. board/kind/source/boundary는 1, edge는 2, collection은 1, kick은 1로 고정하고 supply multiset만 0/1로 바꿨다.

`quotient10-eight-comparison.json`은 비교 출력 v7에서 `validation_passed=true`, `equivalent=true`, contract/result differences 모두 빈 배열을 기록한다. 전체 실행 묶음의 process exit는 0이다. 네 arm 모두 원래 공급 **211,680개 중 211,234개**를 cover하며 확률은 **0.9978930461073318**, 전체 원본 coverage-word SHA256은 **`24d3271c4b4af592b97186ce2a0b542c032b4b98c7e93fcb8774a9a687ba9797`**다. probability·packing·count complete, resource non-truncated, BuildUp 0의 완료 gate도 통과했다. 이 CoverageSummary 비교는 전체 candidate tuple digest나 materialized solution identity SHA를 계산한 실행이 아니며, 앞선 oracle의 identity 해시를 이 결과의 해시로 대신 쓰지 않는다.

| 실행 순서 | supply multiset | 전체 실행 초 | engine 논리 peak bytes | sampled process-group RSS peak bytes |
| --- | ---: | ---: | ---: | ---: |
| 0a | 0 | 36.999719707 | 740,353,210 | 947,355,648 |
| 1a | 1 | 11.142510369 | 331,392,242 | 449,363,968 |
| 1b | 1 | 10.066247978 | 331,385,718 | 439,107,584 |
| 0b | 0 | 36.807843522 | 740,354,334 | 948,150,272 |

Rust comparator의 모드 0 평균은 **36.9037816145초**, 모드 1 평균은 **10.6043791735초**, treatment/reference 평균 비율은 **0.2873521007758564**다. 각 모드 두 표본의 ABBA 비교이며 p95·통계적 유의성·다른 크기의 안정적인 개선율을 추정하지 않는다. 이전 q9의 순차 단일 표본보다 같은 축의 반복 비교를 강화했지만, 여전히 이 8-lock fixture의 실측이다.

| 첫 번째 완료 pair의 내부 계측 | 0a | 1a |
| --- | ---: | ---: |
| policy state | 33,294,716 | 7,049,743 |
| policy action check | 55,993,415 | 12,791,500 |
| kind-union node / edge | 39,868 / 105,178 | 59,431 / 146,395 |
| kind-union member / source visit | 314,671 / 1,065,198 | 220,224 / 662,570 |
| charged kind-union work | 1,736,625 | 23,741,330 |
| 공급 필터 checked / pruned root | 0 / 0 | 520,573 / 483,845 |
| certificate node / edge | 0 / 0 | 45,454 / 128,166 |
| 공급별 alias | 0 | 40,497 |
| worker observation producer retained bytes | 10,219,436 | 10,242,260 |

첫 pair의 stderr 단계 시간은 source 준비 **0.023971773 / 0.024760935초**, freeze **0.000549539 / 0.000638634초**, policy **36.644248965 / 10.859365241999999초**다. source 준비 시간은 전체 Geometry 시간과 같은 항목이 아니다. 필터는 불가능한 kind root를 제외하여 정책 memo와 행동 검사를 줄였지만, certificate·alias의 구성 및 조회도 실제 work로 청구한다. 특히 **총시간 감소와 공통 64M work 상한의 여유는 서로 다른 축**이다. charged kind-union work가 늘었으므로 이 결과만으로 P7P4의 work 거절이 해결됐다고 쓰지 않는다. §3.14의 전체 P7P4 미완료 결과와 함께 평가해야 한다. 공급 multiset의 제품 기본은 계속 0이다.

메모리 표의 engine 값은 `resource_peak_cpu_bytes`, RSS는 각 runtime receipt의 `linux-sampled-process-group-rss`다. receipt는 실행 순서대로 `1795330202530712663-5`, `1795330239567354951-18`, `1795330250775220563-31`, `1795330260868907238-44`이며 모두 kernel peak는 null이다. 두 종류의 peak를 합산하거나 하나를 다른 하나로 대체하지 않는다. 이 pair에서 producer retained bytes 자체는 줄지 않았으므로 전체 메모리 감소를 producer 보유량 감소라고 설명하지 않는다.

### 3.16 q11 P7P4: prefix mask 방문 생략 후에도 정책 work 상한으로 미완료

q11 source `55ec4b711267253536008fec3b1e24bafff12045`는 관측 focused **101 passed / 0 failed / 4.58초**와 release build를 완료했다. 보관 바이너리 SHA256은 `e14b42bcd2d7e291a09e890294d4c6723b90bf44278f7750e9bf119acacf0e4b`다. schema v9, `p7p4 / visible-7`, Auto/9-worker에서 board/kind/source/boundary 1, edge 2, collection 1, supply multiset 1, kick 1을 고정하고 **prefix mask 1 → 0** 순서로 실행했다.

`quotient11-p7p4-mask1.json`과 `quotient11-p7p4-mask0.json`은 모두 `complete=false`, `InvalidProblem { reason: "wasm_observation_physical_work_limit" }`를 기록한다. stderr에서는 두 arm 모두 전체 후보 등록·freeze를 마쳤고 kind union의 추가 work 청구에서 거절된 것을 확인했다. 완료 확률이나 coverage SHA는 없으며 이 두 실패 결과로 완료 정확성 parity 또는 속도 개선을 주장하지 않는다.

| frozen board 및 거절 시점 계측 | mask 0 | mask 1 |
| --- | ---: | ---: |
| candidate factor / board node | 456,459 / 569,195 | 456,459 / 569,195 |
| board edge / suffix root | 1,803,660 / 4,361,665 | 1,803,660 / 4,361,665 |
| prefix visit | 23,789,092 | 17,875,186 |
| edge attempt | 15,714,669 | 15,714,669 |
| source-pruned / dead source prefix | 5,913,906 / 69,615 | 5,913,906 / 69,615 |
| board work, helper 포함 | 62,407,602 | 61,060,991 |
| 정책 kind-union work / 다음 청구 | 1,592,398 / 1 | 2,939,009 / 1 |

| prefix mask helper 계측 | mask 0 | mask 1 |
| --- | ---: | ---: |
| lookup / hit / miss | 0 / 0 / 0 | 456,459 / 455,282 / 1,177 |
| full fallback | 0 | 0 |
| 처리 bitmap word | 0 | 3,776,219 |
| helper work | 0 | 4,416,768 |

mask 1은 기존에 source 제한으로 거절하던 **5,913,906개 prefix**를 방문하기 전에 걸러냈다. helper work는 query cache 구성·조회와 runtime word filter의 실제 추가 work이며 이미 board work에 포함되므로 다시 더하지 않는다. 전체 board work에는 worker별 후보 배분과 합치는 과정의 비용도 포함된다. 따라서 두 실행의 board work 차이를 단순히 `생략한 prefix 수 − helper work`로 동일시하지 않는다. board node·edge 수가 같다는 사실만으로 전체 relation 또는 원본 coverage 해시 동등성을 실측했다고 쓰지 않는다.

| 종료까지의 단계 시간 | mask 0, 초 | mask 1, 초 |
| --- | ---: | ---: |
| observation source 준비 | 1.116042849 | 1.48790713 |
| freeze | 0.404257458 | 0.448128051 |
| 정책 평가, 거절까지 | 0.319310259 | 0.544776494 |
| 전체 실행, 거절까지 | 96.03966115 | 94.133804808 |

이 표는 실패 단계 진단이며 평균·비율·speedup 계산에 쓰지 않는다. 두 arm의 정책 거절 시점도 서로 다르므로 부분 정책 시간과 부분 retained bytes로 완료 solver의 비용을 비교하지 않는다. source 준비 시간은 전체 Geometry 시간과 같은 항목이 아니다.

mask 1의 runtime receipt `1795330958875596166-5`는 sampled process-group RSS peak **5,946,073,088 bytes**, mask 0의 receipt `1795331053126740399-18`은 **5,735,194,624 bytes**를 기록한다. 둘 모두 kernel peak는 null이다. 이는 미완료 실행에서 관측한 process RSS이며 engine owner의 retained bytes와 합산하거나 대체하지 않는다.

이 P7P4 실패 진단과 별도로, 같은 바이너리 8-lock ABBA 완료 결과는 다음 절에 기록한다. 10/11-lock 완주는 아직 pending이다.

### 3.17 q11 실제 8-lock: prefix mask ABBA 동등성 확인, 총시간 개선 미관측

`quotient11-eight-mask0a.json`, `quotient11-eight-mask1a.json`, `quotient11-eight-mask1b.json`, `quotient11-eight-mask0b.json`은 §3.16과 같은 q11 소스·바이너리·CPU에서 fixture만 `eight`으로 바꾸어 0, 1, 1, 0 순서로 완료한 실행이다. prefix mask 이외의 일곱 관측 축과 kick 1을 고정했다. `quotient11-eight-comparison.json`은 비교 출력 v8에서 **`validation_passed=true / equivalent=true`**, contract/result differences 모두 빈 배열을 기록한다.

네 arm의 원래 공급은 **211,680개**, covered count는 **211,234개**, 확률은 **0.9978930461073318**, 전체 coverage-word SHA256은 **`24d3271c4b4af592b97186ce2a0b542c032b4b98c7e93fcb8774a9a687ba9797`**로 같다. 완료·resource non-truncated·BuildUp 0 gate도 통과했다. 이 결과 역시 CoverageSummary의 전체 원본 coverage 비교이며 전체 candidate tuple digest나 solution identity SHA의 비교로 넓혀 쓰지 않는다.

| 실행 순서 | prefix mask | 전체 실행 초 | engine 논리 peak bytes | sampled process-group RSS peak bytes |
| --- | ---: | ---: | ---: | ---: |
| 0a | 0 | 12.597796979 | 331,389,282 | 437,395,456 |
| 1a | 1 | 12.896379998 | 331,539,054 | 429,404,160 |
| 1b | 1 | 10.717690245 | 331,534,470 | 439,889,920 |
| 0b | 0 | 10.833878568 | 331,386,426 | 454,987,776 |

Rust comparator의 mask 0 평균은 **11.715837773499999초**, mask 1 평균은 **11.8070351215초**, treatment/reference 평균 비율은 **1.0077841081246686**이다. **이 완료 비교에서 총시간 개선은 관측하지 못했다.** 각 모드 두 표본의 ABBA이며 한 표본의 빠른 시간만 선택하거나 이 작은 차이를 안정적인 악화율로 일반화하지 않는다.

| producer 계측 | 0a | 1a | 1b | 0b |
| --- | ---: | ---: | ---: | ---: |
| prefix visit | 35,949 | 35,083 | 35,083 | 35,949 |
| board work, helper 포함 | 138,425 | 201,796 | 200,386 | 138,071 |
| helper lookup | 0 | 2,597 | 2,597 | 0 |
| helper hit / miss | 0 / 0 | 1,963 / 634 | 1,977 / 620 | 0 / 0 |
| helper full fallback / bitmap word | 0 / 0 | 0 / 3,759 | 0 / 3,759 | 0 / 0 |
| helper work | 0 | 64,468 | 63,184 | 0 |

mask 1은 8-lock에서 기존 source-pruned prefix **866개**의 방문을 피했지만 helper work가 추가되어 이 실행들의 전체 board work는 증가했다. 네 arm 모두 board node 2,018개, edge 3,790개, suffix root 21,158개다. 첫 pair의 policy state는 7,049,042 / 7,048,423개, action check는 12,790,829 / 12,788,986개로 정책 평가 규모도 크게 줄지 않았다. helper hit/miss와 전체 work는 worker별 배분·cache owner의 영향을 받으므로 동일 모드 반복에서도 모두 같다고 가정하지 않는다.

이 결과는 입력 크기에 따른 비용 차이를 보여준다. 전체 P7P4에서는 많은 불가능 prefix 방문을 생략하고 helper를 포함한 board work도 낮아졌지만 여전히 완주하지 못했다. 8-lock에서는 생략량이 작고 helper 비용이 커서 총시간 이득을 관측하지 못했다. 따라서 현재 제품 기본은 **prefix mask 0**을 유지하며, 10-lock 실패시간으로 이 결정을 뒤집지 않는다.

RSS receipt는 실행 순서대로 `1795331217184340323-5`, `1795331229840880806-18`, `1795331242752648394-31`, `1795331253496687970-44`이며 모두 sampled process-group RSS이고 kernel peak는 null이다. engine 논리 peak와 RSS는 별도 계측으로 유지한다.

후속 q12/q13 통합은 이 q11 보관 바이너리의 실측에 포함되지 않는다. 해당 통합과 출처 기록 문제는 다음 절에 분리한다.

### 3.18 q12/q13 통합과 q13b 정식 실행의 출처 경계

q12 subset import `cd08d4b`, q13 공개 전 memo `c3b8adf`, 소비자 연결을 포함한 통합 소스는 **`abddcccc865472b0cb0c7b4b0ebb7da43dcf984b`**다. `quotient13-observation-tests.log`는 관측 focused **110 passed / 0 failed / 5.11초**, `quotient12-import-tests.log`는 subset import focused **5 passed / 0 failed / 0.01초**를 기록한다. 이 두 검증 범위를 저장소 전체 suite나 10/11-lock 완주로 바꾸지 않는다.

첫 `quotient13-eight-*.json` 실행의 빌드 metadata에는 `compiled_source_commit="abddccc"`가 기록됐다. `quotient13-eight-comparison.json`의 `contract_differences`와 `result_differences`는 빈 배열이지만, 각 실행의 source commit이 요구되는 40자리 또는 64자리 lowercase hex 길이를 충족하지 않아 **`validation_passed=false`, `equivalent=false`**다. 이는 원본 coverage 차이를 발견한 결과가 아니며, 출처 검증 실패를 무시해 동등성 통과로 승격하지 않는다. 이 첫 자료는 예비 기록으로 보존하고 정식 시간 평균·비율·채택 판단에 사용하지 않는다. 예비 memo hit율을 완료 실측 결론으로 발표하지 않는다.

전체 source SHA를 기록해 다시 빌드한 **q13b**의 바이너리 SHA256은 **`557010cb32ba7c70e3b966f8b923d546d2eec406f70572d8ea3db104ef326b5b`**이며 `quotient13b-release-build.log`는 **24.08초**에 release build 완료를 기록한다. q13b는 schema v10을 사용하며 suffix import와 공개 전 memo를 각각 별도 축으로 드러낸다. prefix mask 1·suffix import 1을 고정한 memo 0, 1, 1, 0 ABBA와, memo 1에서 suffix import만 0으로 바꾼 추가 대조를 마쳤다. 정식 8-lock 결과와 같은 바이너리 P7P4 실패는 다음 두 절에 분리한다.

정식 artifact 이름은 `quotient13b-eight-memo0a.json`, `quotient13b-eight-memo1a.json`, `quotient13b-eight-memo1b.json`, `quotient13b-eight-memo0b.json`, `quotient13b-eight-import0.json`, `quotient13b-eight-comparison.json`, `quotient13b-p7p4.json`이다. 이 자료를 예비 `quotient13-*` 결과와 같은 바이너리의 표본처럼 섞지 않는다. 원래 공급·관측 계약에서의 10/11-lock 완주는 여전히 미완료다.

### 3.19 q13b 실제 8-lock: 공개 전 memo 악화와 subset import 공간 효과

§3.18의 전체 source SHA·보관 바이너리에서 다섯 arm이 모두 완료했다. board/kind/source/boundary 1, edge 2, collection 1, supply multiset 1, prefix mask 1, kick 1, Auto/9-worker를 고정했다. `quotient13b-eight-comparison.json`은 비교 출력 v9에서 **`validation_passed=true / equivalent=true`**, contract/result differences 모두 빈 배열을 기록한다. 다섯 실행 및 비교 process exit는 0이다.

모든 arm의 원래 공급 **211,680개 중 211,234개**, 확률 **0.9978930461073318**, 전체 coverage-word SHA256 **`24d3271c4b4af592b97186ce2a0b542c032b4b98c7e93fcb8774a9a687ba9797`**가 같다. 완료·resource non-truncated·BuildUp 0 gate도 통과했다. candidate tuple 전체 digest와 solution identity SHA는 계산하지 않았으므로 이 coverage 비교를 그 집합들의 해시 비교로 확대하지 않는다.

| 실행 순서 | suffix import | 공개 전 memo | 전체 실행 초 | engine 논리 peak bytes | sampled process-group RSS peak bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| memo0a | 1 | 0 | 10.56689978 | 329,632,266 | 427,646,976 |
| memo1a | 1 | 1 | 11.590177861 | 331,261,438 | 523,862,016 |
| memo1b | 1 | 1 | 11.439923092 | 331,221,206 | 456,011,776 |
| memo0b | 1 | 0 | 10.202070691 | 329,635,190 | 423,186,432 |
| import0 | 0 | 1 | 12.156535739 | 333,171,178 | 455,794,688 |

공개 전 memo의 ABBA는 각 모드 두 표본이며, Rust comparator 평균은 **10.3844852355초 → 11.5150504765초**, on/off 평균 비율은 **1.1088706098916772**다. 이 완료 fixture에서 시간 악화를 관측했으므로 제품 기본은 **공개 전 memo 0**을 유지한다. 다른 크기나 다른 CPU에서도 같은 악화율이 나온다는 주장은 하지 않는다.

세 개의 memo 1 실행은 모두 **21,160,524 probes, 9,874 hits, 21,150,650 inserts, 0 full fallbacks**를 기록했다. 조회에 비해 재사용 hit가 적었으며, 기존 normalization을 생략한 횟수가 추가 hash 조회·삽입의 총비용을 상쇄하지 못한 결과다. memo 자체 retained/peak는 **1,081,376 / 1,622,080 bytes**다. inserts는 평가 전체의 누적 계측이며, 초기 관측과 재구성 경계를 지날 때 값을 비우므로 동시에 21,150,650개 entry를 보유했다는 뜻이 아니다. retained bytes는 evaluator 보유량에 이미 포함된다.

suffix import의 별도 대조는 memo 1을 고정했다. off **한 표본 12.156535739초**와 on **두 표본 평균 11.5150504765초**, Rust 평균 비율 **0.9472312444702468**이다. 이 대조는 ABBA가 아니며 표본 수와 실행 위치가 다르므로 안정적인 시간 개선을 확정하지 않는다. 확인한 구조적 효과는 `board_ready`의 중앙 kind arena가 **121,268 nodes → 45,901 nodes**로 줄고 원래 coverage를 보존한 것이다. suffix import의 제품 기본도 아직 0이다.

| subset import 계측, 두 memo 1 실행 | memo1a | memo1b |
| --- | ---: | ---: |
| source node / edge | 182,500 / 504,998 | 182,242 / 504,488 |
| reachable node / edge | 69,902 / 185,041 | 70,130 / 185,562 |
| destination에 새로 넣은 node / edge | 45,901 / 129,366 | 45,901 / 129,366 |
| root visit / node visit / edge visit | 21,637 / 139,804 / 370,082 | 21,691 / 140,260 / 371,124 |
| mapping slot | 547,500 | 546,726 |
| before / after / peak bytes | 2,958,284 / 3,097,232 / 3,895,448 | 2,969,708 / 3,112,800 / 3,940,916 |

source/reachable/inserted 및 visit·slot 계측은 **각 shard import에서 계산한 값을 합산**한 것이다. source와 reachable에는 worker 간 중복이 있을 수 있고 worker 배분에 따라 달라지므로 전체 query의 고유 node 수로 읽지 않는다. destination interning이 중복을 합치므로 reachable 합계가 최종 중앙 arena보다 클 수 있다. before/after/peak bytes는 순차 import들의 합계가 아니라 **각 source/destination/map 공존 순간의 최대 snapshot**이다. peak에는 buffer 성장 시 old/new 공존도 포함하며 engine peak나 process RSS에 다시 더하지 않는다. import 0의 subset 전용 계측 0은 해당 계측 경로를 사용하지 않았다는 뜻이며 full import 비용이 없다는 뜻이 아니다.

RSS receipt는 표 순서대로 `1795331976233625453-5`, `1795331986871269074-18`, `1795331998519079388-31`, `1795332010007509314-44`, `1795332020242683479-57`이다. 모두 sampled process-group RSS이고 kernel peak는 null이다. q14의 별도 수학적 개선은 이 q13b 소스·바이너리에 포함되지 않는다.

### 3.20 q13b P7P4: 중앙 arena 축소 후에도 정책 work 상한으로 미완료

`quotient13b-p7p4.json`은 동일한 전체 source SHA·바이너리에서 모든 관측 축 1, edge 2, kick 1, Auto/9-worker로 실행한 결과다. **77.472324947초 후 `complete=false`, `InvalidProblem { reason: "wasm_observation_physical_work_limit" }`**를 기록했다. receipt `1795332032449708669-71`의 child termination은 **exit 1**이며 timeout이나 supervisor 강제 종료가 아니다. 이 종료시간을 이전 실패 실행과의 speedup으로 비교하지 않는다.

| 등록·freeze 및 정책 거절 시 계측 | 기록값 |
| --- | ---: |
| candidate factor / board node | 456,459 / 569,195 |
| board edge / suffix root | 1,803,660 / 4,361,665 |
| prefix visit / edge attempt | 17,875,186 / 15,714,669 |
| source-pruned / dead source prefix | 5,913,906 / 69,615 |
| frozen 중앙 kind arena node | 1,670,993 |
| base board work | 60,969,274 |
| kind-union node / edge | 28,315 / 43,033 |
| kind-union work / 다음 청구 | 3,030,726 / 1 |
| supply checked / pruned root | 807,163 / 764,575 |
| 공급 certificate node / edge / alias | 78,711 / 210,662 / 55,672 |
| prefix mask lookup / hit / miss | 456,459 / 455,280 / 1,179 |
| prefix mask word / work | 3,776,219 / 4,417,104 |
| 공개 전 memo probe / hit | 545,984 / 899 |
| 공개 전 memo insert / full fallback | 65,536 / 479,548 |
| 공개 전 memo retained / peak bytes | 4,325,408 / 6,488,128 |

전체 후보 등록·freeze는 끝났지만 base와 kind-union 청구가 공통 **64,000,000**에 도달해 다음 1 step을 거절했다. 공개 전 memo는 이 큰 fixture에서 보유 한도에 도달해 full fallback을 기록했다. 그 fallback은 기존 정확한 평가를 계속하는 경로이며 정상 확률 0이나 성공을 뜻하지 않는다. 전체 coverage·완료 확률 결과는 없다.

stderr의 단계 시간은 observation source 준비 **0.583749648초**, freeze **0.94486677초**, 거절까지 policy **0.566798699초**다. source 준비를 전체 Geometry 시간과 동일시하거나 부분 policy 시간을 완료 solver 시간으로 사용하지 않는다. `symbolic_recycle` 진단도 기록됐지만 worker별 선택된 진행 로그의 줄 수를 전체 recycle 호출 횟수로 합산하지 않는다. 중앙 arena가 작아진 것과 누적 physical work의 완료조건은 별개이며, 기존 work를 지우거나 다시 시작하지 않았다.

같은 receipt의 sampled process-group RSS peak는 **5,920,059,392 bytes**, kernel peak는 null이다. 이 값은 전체 미완료 실행의 process 메모리이며 중앙 arena snapshot 또는 memo retained bytes를 대신하지 않는다. q13b까지의 실제 10/11-lock 완주는 여전히 미완료다. 후속 q14의 구현·회귀 상태는 다음 절에 기록하며 q13b 실측에는 포함하지 않는다.

### 3.21 q14 존재성 producer의 구현·회귀 범위

q14의 존재성 producer는 `a4bbd22ae3f5c0f239b55330f39b5121775ca7d0`, 명시적 strict-prefix 실험의 선행 admission 경로를 유지하는 후속 소스는 **`188e583b7e59767d3f95d88ad430585d26cf0d75`**다. 보관 바이너리 SHA256은 **`998970d1f385fa319fb17ae40701d9837d633e60cfecbe234260da684f34a897`**이며 `quotient14-release-build.log`에서 **1분 44초**에 release build 완료를 확인했다. runtime receipt는 `1795333032103666271-5`다.

이 producer의 추가 권한은 실제 전체 fresh Standard7Bag, 7..11 locks, 실제 materialized 길이 N=n+1, cursor 0, 처음에 빈 Hold와 Hold 허용, terminal supply target 없음, **`projects_unplaced_lookahead=false`와 `projects_standard_bag_lookahead=false`**다. 기존 board family 완료 권한·CountUnique/CoverageSummary·evidence/score/filter 배제도 유지한다. kind 개수 상한과 exact physical relation의 productive 여섯-lock distinct prefix로 원래 공급에서 성공할 수 있는 후보인지 판단하며, 적용되지 않는 요청과 실제 거절·등록을 typed 결정으로 구분한다. 전체 전칭 논증과 source witness 구성은 [증명 문서 B15](see7-quotient-pruning-proof-2026-11-22.md)를 따른다. 단순히 후보의 물리 언어가 비어 있지 않다는 판정을 성공 공급의 존재로 대체하지 않는다.

첫 `quotient14-observation-tests.log`는 테스트 module의 `PrefixSuffixCompilation`, `EMPTY_REFERENCE` import 누락으로 **9개 컴파일 오류**를 기록했다. 이는 테스트가 실행된 뒤 실패한 결과가 아니다. import 수정 뒤 `quotient14-observation-tests-fixed.log`는 **120 passed / 0 failed / 5.16초**, receipt `1795332929665219718-5`다. `quotient14-suffix-tests.log`는 **3 passed / 0 failed / 0.00초**, receipt `1795332995494746558-5`다.

120개 관측 검증에는 fresh-bag 관련 7개 테스트가 포함된다. 구성적 witness 회귀는 일곱 kind 이름 회전마다 5,018개 유효 tail을 만들어 **35,126개 구성 word**를 실제 Hold automaton으로 실행하며, 별도 12개 경계 word는 독립 bag product와 비교한다. suffix 검증은 필요한 root들의 언어 보존, empty relation과 없는 root0의 구분, operation-word 경계를 넘는 최대 domain을 확인한다. 이 수치는 전체 공급·geometry의 큰 fixture를 완주했다는 뜻이 아니며 **구현 연결을 확인하는 회귀 증거**다. predicate의 전칭 증명은 이 표본 수에 의존하지 않는다.

비교기도 v11 입력·출력 v10으로 컴파일했다. 기존 v10 입력의 `quotient13b-eight-comparison-v10.json`은 **`validation_passed=true / equivalent=true`**와 빈 contract/result differences를 기록한다. 이는 기존 schema의 호환성 검증이며 q14 source-existence 경로의 실제 완료 비교는 다음 절에 별도로 기록한다. 제품 기본은 source existence 0이다.

### 3.22 q14 실제 8-lock: source-existence ABBA 동등성 확인

`quotient14-eight-source0a.json`, `quotient14-eight-source1a.json`, `quotient14-eight-source1b.json`, `quotient14-eight-source0b.json` 순서의 ABBA 네 실행은 같은 source `188e583b`, 보관 바이너리·Auto/9-worker·kick 1에서 완료했다. board/kind/source prefix/boundary/collection/supply multiset/prefix mask/suffix import는 1, edge는 2, 공개 전 memo는 0으로 고정하고 source existence만 바꿨다. typed metadata의 실제 N은 **9**, 두 projection은 false다.

`quotient14-eight-comparison.json`의 비교 출력 v10은 **`validation_passed=true / equivalent=true`**와 빈 contract/result differences를 기록한다. 모든 arm의 원래 공급 **211,680개 중 211,234개**, 확률 **0.9978930461073318**, 전체 coverage-word SHA256 **`24d3271c4b4af592b97186ce2a0b542c032b4b98c7e93fcb8774a9a687ba9797`**가 같으며 완료·resource non-truncated·BuildUp 0 gate도 통과했다.

| 실행 순서 | source existence | 전체 실행 초 | engine 논리 peak bytes | sampled process-group RSS peak bytes |
| --- | ---: | ---: | ---: | ---: |
| source0a | 0 | 10.888799944 | 329,617,210 | 435,396,608 |
| source1a | 1 | 11.09268622 | 329,583,882 | 395,067,392 |
| source1b | 1 | 10.865473308 | 329,592,490 | 396,615,680 |
| source0b | 0 | 10.324721902 | 329,626,214 | 455,954,432 |

Rust comparator의 off 평균은 **10.606760923초**, on 평균은 **10.979079764초**, on/off 평균 비율은 **1.0351020300827798**다. 각 모드 두 표본의 ABBA이며 이 8-lock 비교에서 총시간 개선은 관측하지 못했다. 안정적인 악화율이나 다른 크기의 성능을 추정하지 않으며 제품 기본은 source existence 0을 유지한다.

| source existence 1의 두 실행에서 같은 계측 | 기록값 |
| --- | ---: |
| 판정 candidate / kind rejection | 27,987 / 0 |
| relation rejection / accepted candidate | 25,390 / 2,597 |
| prefix check / language edge | 349,140 / 359,489 |

새 producer의 accepted **2,597개**는 같은 실행의 board 후보 수 및 모든 source 0 대조의 board 후보 수 **2,597개**와 같다. 비교기의 네 교차 pair count check도 통과했다. 이 수치 비교는 **후보 수 진단**이며 전체 candidate tuple 집합의 해시나 전 경로 증명이 아니다. 이번에도 candidate-set digest와 solution identity SHA는 미계산이며, 정확한 producer 교체의 일반 근거는 B15의 논증이다.

`coverage_row_count`는 **2,597 → 0**, `pattern_verified_execution_count`는 **12,872,888 → 0**으로 바뀌었다. 실제로 생략한 후보별 oracle 단계의 진단값이며 최종 성공 coverage가 없어졌다는 뜻이 아니다. 첫 pair의 policy states는 7,048,801 / 7,049,779, action checks는 12,788,680 / 12,791,171로 최종 정책 평가 규모도 크게 줄지 않았다. source 0에서 새 producer 계측이 0인 것은 그 producer를 사용하지 않았다는 의미다.

RSS receipt는 표 순서대로 `1795333183980320682-5`, `1795333194925218293-18`, `1795333206082891556-31`, `1795333216973460720-44`다. 모두 sampled process-group RSS이고 kernel peak는 null이며 engine 계측과 합산하지 않는다.

### 3.23 q14 P7P4: 존재성 producer 적용 후 정책 work 상한

`quotient14-p7p4.json`은 같은 source·보관 바이너리, N=11·두 projection false·source existence 1·공개 전 memo 0에서 **33.97099872초 후 `complete=false`, `wasm_observation_physical_work_limit`**를 기록한다. 전체 candidate 등록·freeze는 끝났고 다음 kind-union work 청구를 거절했다. 이전 실패시간과의 비율이나 완료 solver speedup은 계산하지 않는다.

| q14 P7P4 source·board 계측 | 기록값 |
| --- | ---: |
| source candidate / kind rejection | 29,856,840 / 0 |
| source relation rejection / accepted | 29,400,381 / 456,459 |
| source prefix check / language edge | 89,180,737 / 87,228,900 |
| board candidate / node | 456,459 / 569,195 |
| board edge / suffix root | 1,803,660 / 4,361,665 |
| 중앙 kind arena node | 1,670,993 |
| base board work | 60,965,451 |
| kind-union work / 다음 청구 | 3,034,549 / 1 |
| kind-union node / edge | 28,340 / 43,067 |

source producer가 원래 oracle-positive 후보 456,459개를 등록했지만, 누적 board·kind work는 기존 **64,000,000**에 도달했다. source prefix·language edge 계측은 해당 compiler의 구조 작업량이며 physical work 상한을 대체하거나 여기에 다시 합산하는 값이 아니다. 전체 source 판정과 등록을 통과한 사실을 최종 정책 평가 완료로 바꾸지 않는다. 완료 coverage·확률 결과는 없다.

stderr의 observation source 준비는 **1.028949311초**, freeze는 **0.373931483초**, 거절까지 policy는 **0.63045077초**다. source 준비는 전체 Geometry 시간이 아니며 부분 policy 시간을 완료 solver 시간으로 사용하지 않는다. receipt `1795333227372876420-58`은 **exit 1**, sampled process-group RSS peak **589,479,936 bytes**, kernel peak null, supervisor termination reason null을 기록한다. 이는 해당 미완료 실행의 실측이며 완주 메모리 요구량의 상한을 뜻하지 않는다.

q14의 `eleven` 최종 실패는 다음 절에 별도로 기록한다. q15 source filter는 이 q14 측정에는 적용되지 않았다.

### 3.24 q14 11-lock: worker producer의 physical node 상한으로 미완료

`quotient14-eleven.json`은 q14와 같은 source·바이너리, `eleven / visible-7`, N=12·두 projection false·source existence 1·공개 전 memo 0에서 **578.629917026초 후 `complete=false`, `wasm_observation_physical_node_limit`**를 기록한다. 최종 stderr는 한 worker의 `source_producer_failed`를 명시한다. 일부 worker의 생산 종료 기록이 있더라도 전체 board-ready·freeze·정책 평가가 완료된 결과는 아니다.

| 실패한 worker의 부분 source·kind 계측 | 기록값 |
| --- | ---: |
| source candidate / kind rejection | 42,916,800 / 0 |
| source relation rejection / accepted | 42,546,204 / 370,595 |
| source prefix check / language edge | 58,538,838 / 64,981,556 |
| 해당 kind arena node / edge | 3,205,641 / 8,525,728 |
| kind arena local live / retained bytes | 95,010,091 / 147,349,180 |

이 표는 **실패한 worker 하나의 중간 상태**이며 전체 query의 최종 후보 수나 전체 arena 크기가 아니다. 진행 로그의 부분 수치들을 최종 집계처럼 합산하지 않는다. 거절한 physical board owner의 고정 node 상한은 **1,000,000**이며 위 표의 kind arena node 수와 서로 다른 계측이다. 기존 상한을 올려 통과시킨 실행이 아니다.

runtime receipt `1795333261400549240-71`은 **child exit 1**, sampled process-group RSS peak **2,476,457,984 bytes**, kernel peak null, supervisor termination reason null을 기록한다. 설정된 timeout은 600초였지만 보고된 최종 원인은 내부 physical node 상한이다. 이를 timeout 또는 OOM으로 표시하지 않는다. 전체 coverage·완료 확률은 없으며 이 실패 종료시간으로 다른 solver의 speedup을 계산하지 않는다.

q14까지 실제 10/11-lock 완주는 아직 달성하지 못했다. 별도 q15 source filter의 구현과 이후 검증은 q14의 미완료 결과와 구분한다.

### 3.25 q15 source-qualified prefix: focused 검증과 release

`55167107eee0fa66a880a355b430cb45f5d89979`는 실제 fresh-bag 공급 계약에 맞는 prefix만 보유하는 source-qualified compiler와 소비자 연결을 반영한다. 이 변경은 q14 보관 바이너리의 어느 실행에도 포함되지 않는다. 제품 기본 `source_filter_mode=0`과 local A/B mode 1을 구분한다. 보관 바이너리 SHA256은 **`7f2a1dd051fb04f8ad46d027ca46a3e8f6b97ccce58bd3006eadf71e6a79663e`**다. release receipt `1795334202241460770-5`는 정상 exit 0과 감독 실행시간 **104,590ms**를 기록한다.

| 검증 로그 | 결과 | runtime receipt |
| --- | --- | --- |
| `quotient15-observation-tests.log` | 124 passed, 0 failed, 0 ignored, 1,050 filtered out; 5.14초 | `1795334116507195208-5` |
| `quotient15-source-filter-tests.log` | 7 passed, 0 failed, 0 ignored, 1,167 filtered out; 0.04초 | `1795334179100727486-5` |

직접 filter 회귀는 독립 word filter와 full/boundary suffix의 일치, 완전 permutation domain의 word 경계, 실제 geometric fresh-bag word 보존, 영구적인 종류별 1개 cap과의 구분, dead early branch 제거 후 다른 경로 보존, six-kind path가 없는 empty 결과, 잘못된 scope/domain 및 취소를 확인한다. 이들은 구현 연결의 유한 회귀이며 허용된 모든 경로의 동치 증명을 대신하지 않는다.

v12 입력을 지원하는 비교 출력 v11은 실제로 컴파일했고 `quotient14-eight-comparison-v11.json`에서 기존 v11 네 arm의 `validation_passed=true / equivalent=true`를 확인했다. 실제 q15 v12 입력 비교와 P7P4 미완료 결과는 다음 두 절에 별도로 기록한다.

### 3.26 q15 실제 8-lock: source-filter ABBA 완료와 비용 감소

`quotient15-eight-filter0a.json`, `quotient15-eight-filter1a.json`, `quotient15-eight-filter1b.json`, `quotient15-eight-filter0b.json` 순서로 같은 q15 source·보관 바이너리·Auto/9-worker·kick 1을 사용했다. board/kind/source prefix/boundary/collection/supply multiset/prefix mask/suffix import/source existence는 1, edge는 2, 공개 전 memo는 0이며 source filter만 0/1로 바꿨다. 실제 N은 **9**, 두 lookahead projection은 false다.

`quotient15-eight-comparison.json`은 **`validation_passed=true / equivalent=true`**, 빈 contract/result differences를 기록한다. 모든 arm은 원래 **211,680개 중 211,234개**, 확률 **0.9978930461073318**, coverage SHA256 **`24d3271c4b4af592b97186ce2a0b542c032b4b98c7e93fcb8774a9a687ba9797`**가 같다. 완료·resource non-truncated·BuildUp 0 gate도 통과했다. mode 1의 accepted 2,597개와 filter calls 2,597개가 같고 모든 mode 0 대조의 board 후보 2,597개와도 같다. 이 count check를 미계산 candidate-set digest나 전 경로 증명으로 표현하지 않는다.

| 실행 순서 | source filter | 전체 실행 초 | engine 논리 peak bytes | sampled process-group RSS peak bytes |
| --- | ---: | ---: | ---: | ---: |
| filter0a | 0 | 10.866708117 | 329,575,846 | 399,114,240 |
| filter1a | 1 | 9.957628959000001 | 326,454,450 | 251,899,904 |
| filter1b | 1 | 10.080531983 | 326,450,234 | 390,807,552 |
| filter0b | 0 | 10.577534358 | 329,581,798 | 397,348,864 |

Rust comparator의 off 평균은 **10.7221212375초**, on 평균은 **10.019080470999999초**, on/off 비율은 **0.9344308135557023**다. 각 모드 두 표본의 ABBA에서 총시간 감소를 관측했다. 이 값은 해당 완료 fixture의 기술 통계이며 안정적인 개선율·p95·다른 크기의 성능을 추정하지 않는다. 제품 기본 source filter는 0으로 유지한 상태의 실험이다.

| 같은 8-lock 실행의 구조·정책 계측 | filter 0 | filter 1 |
| --- | ---: | ---: |
| board node / edge | 2,018 / 3,790 | 1,954 / 3,679 |
| suffix root / prefix visit | 21,158 / 35,083 | 18,941 / 32,139 |
| edge attempt | 37,426 | 33,738 |
| 중앙 kind arena node | 45,901 | 29,672 |
| source language edge | 359,489 | 236,886 |
| kind-union work | 23,741,330 | 22,526,724 |
| policy state, 첫 pair | 7,049,254 | 6,482,428 |
| policy action check, 첫 pair | 12,789,794 | 11,754,423 |

새 filter의 두 on 실행은 모두 calls **2,597**, mask words **103,880**, domain words **175,228**, prefix checks **13,731**을 기록한다. 이들은 source compiler의 구조 작업량이며 기존 physical 64M budget에 더하거나 budget을 재정의하지 않는다. on에서는 source certificate가 보장하는 중복 q5/q11 cap 검사를 실제로 생략하여 source-pruned prefix와 prefix-mask work가 0이다. source 0과 비교하여 가지치기가 사라졌다고 해석하지 않는다. board work는 첫 pair에서 197,980 / 124,589이며 shard 배치 영향을 받는 수치와 compiler 구조 계측을 서로 빼서 단일 비용 절감량으로 만들지 않는다.

RSS receipt는 표 순서대로 `1795334338711546258-5`, `1795334349605174249-18`, `1795334359605574094-31`, `1795334369708929639-44`다. 모두 exit 0, kernel peak null이다. engine 논리 peak와 sampled process-group RSS는 다른 계측이며 합산하지 않는다.

### 3.27 q15 P7P4: 공급 제한으로 축소한 뒤에도 정책 work 상한

`quotient15-p7p4.json`은 같은 source·보관 바이너리, N=11·두 projection false·source filter 1에서 **35.667625777초 후 `complete=false`, `wasm_observation_physical_work_limit`**를 기록한다. 전체 candidate 등록과 freeze를 완료한 뒤 kind-union의 다음 work 1 청구를 거절했다. 이전 실패 종료시간과의 비율이나 완료 solver speedup은 계산하지 않는다.

| q15 P7P4의 완료된 등록·부분 정책 계측 | 기록값 |
| --- | ---: |
| source candidate / relation rejection | 29,856,840 / 29,400,381 |
| accepted / board candidate | 456,459 / 456,459 |
| source prefix check / language edge | 89,180,737 / 33,103,774 |
| board node / edge | 527,568 / 1,683,722 |
| suffix root / prefix visit | 3,906,552 / 15,315,266 |
| edge attempt / 중앙 kind arena node | 13,160,074 / 868,853 |
| base board work | 50,068,673 |
| kind-union work / 다음 청구 | 13,931,327 / 1 |
| kind-union node / edge | 105,782 / 176,500 |
| supply root checked / pruned | 3,425,827 / 3,231,617 |

허용된 원래 공급에 맞는 prefix만 보유하여 실제 board·kind 표현은 줄었지만 누적 board·kind work가 다시 기존 **64,000,000**에 도달했다. 이 실행에서 q5/q11 중복 cap 검사 계측은 0이며 source filter의 구조 작업량을 누락된 physical work처럼 더하지 않는다. 정확한 source 제한과 최종 정책 평가 완료는 별도 조건이다. 후보 수 일치만으로 전체 candidate-set 해시를 검증했다고 말하지 않는다.

stderr의 observation source 준비는 **1.548491609초**, freeze는 **0.40004499초**, 거절까지 policy는 **3.448931921초**다. source 준비는 전체 Geometry 시간이 아니며 policy는 미완료 부분 시간이다. receipt `1795334380309101019-58`은 **child exit 1**, sampled process-group RSS peak **851,693,568 bytes**, kernel peak null, supervisor termination reason null을 기록한다. 이 peak는 실패 실행의 실측이며 완주 메모리 요구량의 상한이 아니다. q15 11-lock의 최종 실패는 다음 절에 별도로 기록하며 전체 10/11-lock 완료 확률은 아직 없다.

### 3.28 q15 11-lock: 전체 worker 종료 후 coordinator merge/import의 node 상한

`quotient15-eleven.json`은 같은 q15 source·보관 바이너리, `eleven / visible-7`, N=12·두 projection false·source filter 1에서 **578.288882526초 후 `complete=false`, `wasm_observation_physical_node_limit`**를 기록한다. stderr는 **아홉 worker의 `board_worker_done` 다음 두 차례 coordinator suffix import**를 기록한 뒤 종료한다. 따라서 q14처럼 worker producer 안에서 먼저 거절된 실행과 구분한다. 이번 실행은 worker 생산 종료 후 coordinator의 merge/import 단계까지 도달했으나 전체 board-ready·freeze·정책 평가를 완료하지 못했다.

| coordinator의 마지막 두 kind subset import | 첫 번째 | 두 번째 |
| --- | ---: | ---: |
| 해당 source arena node / edge | 831,872 / 1,437,835 | 1,055,646 / 1,820,813 |
| 새로 삽입한 kind node / edge | 831,872 / 1,437,835 | 682,981 / 1,232,189 |
| import 후 bytes / 공존 peak bytes | 71,453,380 / 80,833,740 | 118,655,032 / 142,489,028 |

이 표는 coordinator import의 두 snapshot이며 전체 query의 완료 집계가 아니다. kind arena node와 physical board owner의 **1,000,000-node 상한**도 별개다. 개별 worker의 candidate·accepted 수는 producer 계측이며 최종 성공 coverage가 아니다.

runtime receipt `1795334416018867031-71`은 **child exit 1**, 감독 실행시간 **578,351ms**, sampled process-group RSS peak **1,612,255,232 bytes**, kernel peak null, supervisor termination reason null을 기록한다. 600초 timeout은 발동하지 않았으며 OOM으로 표시할 근거도 없다. 이 실패시간은 q14와의 speedup 비교값이 아니다. 원래 11-lock 공급에 대한 최종 coverage·완료 확률은 아직 없다.

추가로 독립 Rust `worker_work_bound.rs`가 실제 stderr와 동결 source·binary metadata를 읽어 `quotient15-eleven-work-lower-bound.json`을 만들었다. 아홉 worker 완료 기록의 합은 geometry candidate **414,005,053**, accepted candidate **2,008,622**다. 이는 worker producer 집계이며 완성된 coverage나 candidate-set digest가 아니다.

| 각 worker의 공통 accepted milestone | 기록된 worker 수 | Rust가 합산한 producer work |
| --- | ---: | ---: |
| 65,536 | 9 | 38,954,771 |
| 131,072 | 9 | **77,863,423** |

하계의 전제는 각 worker가 생산 중 하나의 `BoardActionFamily`를 소유하고 candidate count·work를 reset하지 않는다는 점, 양의 65,536 배수 milestone마다 정확히 한 번 기록한다는 점, 이 실행의 worker가 아홉을 넘지 않는다는 점, coordinator merge가 각 source의 누적 work에 추가 merge work를 청구한다는 점이다. 이 조건 아래 아홉 worker가 131,072 accepted에 도달했을 때의 합은 이후 전체 producer work의 하계이며 이미 **64,000,000**을 넘는다. 최종 오류는 node-limit이지만 그 node 병목을 해소해도 현재 생산 기록의 work를 그대로 merge하는 경로에는 추가 work 병목이 남는다.

이 하계는 **이번 관측 실행의 producer 표현과 작업 분할**에 한정된다. 정책 평가만 빠르게 하거나 이미 생성한 history를 merge 뒤에 압축해도 앞서 누적된 이 work를 없애지는 못한다. 다른 알고리즘이나 producer 단계의 공유·가지치기·작업 제거까지 불가능하다는 증명이 아니며, resource 상한을 올리거나 work를 reset할 권한도 만들지 않는다.

### 3.29 q15 P7P4 multiset 0 대조: freeze 후 SIGKILL, 완료 결과 없음

`quotient15-p7p4-multiset0.stderr`와 receipt의 child command는 같은 q15 보관 바이너리·P7P4 VisibleSeven에서 **`CLEARRA_OBSERVATION_SUPPLY_MULTISET=0`만 변경**했음을 보여준다. source existence/filter, board/kind/boundary/suffix import 등의 축은 §3.27과 같다. `quotient15-p7p4-multiset0.json`은 **0 bytes**이며 완료 coverage·확률이나 최종 정책 오류 JSON이 없다.

마지막 engine 단계 기록은 `board_ready`다. candidate **456,459**, board node **527,568**, edge **1,683,722**, suffix root **3,906,552**, 중앙 kind arena node **868,853**를 보유했으며 base board work는 **49,921,515**다. observation source 준비 **1.478047109초**와 freeze **0.358019387초**가 기록되어 freeze 완료는 확인되지만, 정책 완료 또는 구체적인 내부 종료 원인은 기록되지 않았다.

receipt `1795335027694694262-5`는 **179,251ms 후 SIGKILL(signal 9)**를 기록했고 호출자는 **exit 137**을 관측했다. sampled process-group RSS peak는 **7,341,846,528 bytes**, kernel peak null, supervisor termination reason null, memory pressure events 0이다. kernel OOM 근거는 확인되지 않았으며 설정된 600초 timeout이 발동한 결과도 아니다. RSS 증가만으로 OOM을 확정하지 않는다.

공급 multiset 필터를 끈 대조에서도 완료 결과를 얻지 못했다. 두 미완료 실행의 종료시간 비율을 계산하지 않으며, 빈 JSON을 성공 확률 0이나 정상적인 실패 큐 집합으로 처리하지 않는다.

### 3.30 q16 보드별 multiset group index: 정확성 계약과 focused 검증

소스 **`86aa5b8e255c53636bfacc21a04c4efa706c2759`**는 같은 frozen board의 suffix root를 exact uniform-multiset certificate로 묶는 optional index를 연결한다. 새로운 실제 공급 개수 문맥마다 모든 root를 재검사하던 비용을 줄이는 변경이다. 원래 alias hit당 1 work는 유지하며 unknown certificate의 root는 보존한다. Index 구축·key 조회·실제 root 방문·eviction은 기존 kind-union work에 청구하고 group member도 기존 8M 상한에 포함한다.

[증명 문서 B17](see7-quotient-pruning-proof-2026-11-22.md)은 고정 잔여 길이와 실제 관측 개수로 선택한 group들의 합집합에 unknown root를 더한 집합이 기존 scalar 판정의 결과와 같음을 보인다. 원래 root ID·same-kind NFA child·accepting/depth·Hold/EOF 계약을 그대로 유지하는 것이 적용 전제다. Index fallback과 eviction은 기존 exact 경로로 돌아가며 이미 사용한 work를 reset하지 않는다.

`quotient16-observation-tests.log`는 **129 passed / 0 failed / 0 ignored / 1,050 filtered out / 4.99초**, receipt `1795335289936487066-5`다. 다섯 직접 group 회귀는 작은 공급 전 영역에서 scalar root union과의 동치, 최대 일곱 remainder key와 zero depth, unknown NFA root·member cap fallback, 필수 union 전 optional member eviction, depth/owner/work/cancellation 거절을 확인한다. 기존 board 소비자 회귀도 group mode를 포함하도록 확장했다. 이는 유한 구현 회귀이며 B17의 전칭 root 집합 논증과 별개다.

`quotient16-release.log`는 **1분 43초**에 release build를 완료했고 receipt는 `1795335360756455208-5`다. 보관 바이너리 SHA256은 **`2ea39e9bbf51377ae162888bfe976768c7c34293382f7a595f0cd8f8e0aa6c13`**다. 비교 출력 v12의 컴파일과 기존 q15 네 arm backward comparison도 통과했으며 실제 v13 결과는 아래 두 절에 기록한다. 제품 기본은 group mode 0이다. q16은 q15의 worker producer를 바꾸지 않으므로 §3.28의 **관측 producer work 하계 77,863,423**이 남아 있다. 이 변경만 검증하기 위해 11-lock의 동일한 600초 실행을 반복하지 않았으며 11-lock 완료를 주장하지 않는다.

### 3.31 q16 실제 8-lock: group ABBA 동등성 확인, 총시간 개선 미관측

`quotient16-eight-groups0a.json`, `quotient16-eight-groups1a.json`, `quotient16-eight-groups1b.json`, `quotient16-eight-groups0b.json` 순서의 ABBA 네 실행은 같은 q16 source·바이너리·Auto/9-worker·kick 1을 사용한다. source existence/filter를 포함한 q15의 다른 축을 고정하고 group mode만 바꿨다. 실제 N=9·두 projection false·공개 전 memo 0·edge 2다.

`quotient16-eight-comparison.json`은 **`validation_passed=true / equivalent=true`**, 빈 contract/result differences를 기록한다. 원래 공급 **211,680개 중 211,234개**, 확률 **0.9978930461073318**, coverage SHA256 **`24d3271c4b4af592b97186ce2a0b542c032b4b98c7e93fcb8774a9a687ba9797`**가 모든 arm에서 같다. 완료·resource non-truncated·BuildUp 0와 group 생성 여부 gate를 통과했고 accepted·board 후보 **2,597개**도 네 교차 pair에서 각각 같았다. 이 count 진단을 전체 candidate-set 해시로 해석하지 않는다.

| 실행 순서 | multiset groups | 전체 실행 초 | engine 논리 peak bytes | sampled process-group RSS peak bytes |
| --- | ---: | ---: | ---: | ---: |
| groups0a | 0 | 9.795392985 | 326,435,202 | 252,063,744 |
| groups1a | 1 | 9.383856427 | 327,620,473 | 361,140,224 |
| groups1b | 1 | 10.711133416 | 327,615,185 | 391,430,144 |
| groups0b | 0 | 9.848705798 | 326,473,234 | 252,141,568 |

Rust comparator의 off 평균은 **9.822049391499998초**, on 평균은 **10.0474949215초**, on/off 비율은 **1.0229530030866167**다. 각 모드 두 표본의 ABBA에서 총시간 개선을 관측하지 못했으며 group 기본은 0을 유지한다. 이 비교로 안정적인 악화율이나 다른 크기의 시간을 추정하지 않는다.

| group 1의 두 실행에서 같은 계측 | 기록값 |
| --- | ---: |
| index build / 구축 root visit | 1,815 / 18,546 |
| key lookup / matched root visit | 217,749 / 34,722 |
| unknown root visit / fallback | 0 / 0 |
| retained group member / eviction | 18,546 / 0 |
| group work, kind work에 이미 포함 | 487,829 |

Kind-union work는 **22,526,724 → 21,921,195**로 줄었지만 총시간은 감소하지 않았다. Group work를 kind work에 다시 더하지 않는다. 첫 pair의 policy states **6,482,229 / 6,482,500**, action checks **11,754,089 / 11,754,608**는 각각의 실행 진단값이다. 이 작은 차이를 결과 의미 차이나 exact 정책 평가량의 일치 증명으로 사용하지 않는다. Board node **1,954**, edge **3,679**, suffix root **18,941**, 중앙 kind arena node **29,672**는 모든 arm에서 같다.

RSS receipt는 표 순서대로 `1795335517915531051-5`, `1795335527801184459-18`, `1795335537247631791-31`, `1795335547994326751-44`이며 모두 exit 0·kernel peak null이다. 논리 peak와 sampled RSS를 분리해 기록한다.

### 3.32 q16 P7P4: 첫 초기 관측 분기 후 공통 work 상한

`quotient16-p7p4-groups1.json`은 같은 q16 source·바이너리, N=11·두 projection false·group 1에서 **38.250364388초 후 `complete=false`, `wasm_observation_physical_work_limit`**를 기록한다. 전체 candidate **456,459개**의 등록·freeze는 완료했으나 최종 coverage·확률은 없다. 실패 종료시간을 q15와의 속도 개선율로 사용하지 않는다.

| q16 P7P4 부분 정책·group 계측 | 기록값 |
| --- | ---: |
| base board work / kind-union work | 50,002,445 / 13,997,555 |
| kind-union node / edge | 130,719 / 237,094 |
| group build / 구축 root visit | 7,377 / 109,781 |
| group key lookup / matched root visit | 1,255,681 / 195,803 |
| unknown root visit / fallback / eviction | 0 / 0 / 0 |
| retained group member / group work | 109,781 / 2,476,974 |

Base와 kind work는 기존 **64,000,000**에 도달했고 다음 1 work 청구에서 거절했다. Group work는 kind work에 이미 포함된 부분량이다. Scalar supply root checked/pruned 진단이 0인 것은 group index로 선택한 경로에서 전체 root의 재검사를 생략했기 때문이며 공급 필터가 없었다는 뜻이 아니다.

진행 로그는 **initial observations 1/5,040 완료 시점**의 policy states **2,719,907**, action checks **5,671,738**를 기록했다. 이 한 기록으로 두 번째 초기 관측의 완료 여부나 전체 정책 값을 추정하지 않는다. observation source 준비는 **1.388559215초**, freeze는 **0.359564022초**, 오류까지 policy는 **5.7407702369999996초**다. 모두 해당 단계의 부분 계측이며 전체 Geometry 시간과 구분한다.

Receipt `1795335557897797978-58`은 **child exit 1**, sampled process-group RSS peak **937,930,752 bytes**, kernel peak null, supervisor termination reason null을 기록한다. 이 미완료 실행의 메모리 계측을 완주 필요량의 상한으로 사용하지 않는다. q17 정책 branch-bound 변경은 이 q16 보관 바이너리에 포함되지 않았다.

### 3.33 q17 exact incumbent branch-bound: 정확성 계약과 focused 검증

소스 **`7108df03b50df28c8a04eae7dd71f1e2f22dd76d`**는 현재 정보집합 안의 행동을 평가할 때 이미 정확히 계산한 incumbent 값과 비교한다. 아직 평가하지 않은 관측 분기가 모두 성공한다고 놓은 원래 subtree cardinality 상계로도 incumbent를 이길 수 없으면 해당 행동의 남은 계산을 중단한다. 같은 값에서도 앞서야 하는 tie는 계속 계산하며 첫 legal zero-valued 행동을 거절 상태로 바꾸지 않는다.

[증명 문서 B18](see7-quotient-pruning-proof-2026-11-22.md)은 서로 겹치지 않는 원래 terminal ID 분기들의 합과 정확한 incumbent를 이용한 상계를 제시한다. 첫 구현의 적용 범위는 양의 동일 가중치 certificate·고정 horizon·terminal supply target 없음이다. 비균등/0 가중치·가변 길이·terminal target 등은 기존 경로를 유지한다. Typed `Dominated`는 정확한 0이나 `REJECT`가 아니며 기존 memo와 공개 전 cache에 저장하지 않는다. 실제로 방문한 child의 값과 부모가 확정한 값만 exact memo에 들어가고 원래 supply transition·tie·ID 재생 계약을 유지한다.

`quotient17-observation-tests.log`는 **135 passed / 0 failed / 0 ignored / 1,050 filtered out / 5.07초**, receipt `1795336132384115248-5`다. 여섯 직접 회귀는 분리된 reveal과 중복 ID·replay, raw tie에서 먼저 선택할 동일 count와 첫 zero action, Hold/EOF·projected last draw, partial action의 memo 배제와 fork reset, weighted/variable/terminal-target fallback, dominated 반환 전 child·cancellation 처리를 확인한다. 이 유한 검증은 B18의 전칭 상계 논증을 대신하지 않는다.

`quotient17-release.log`는 **1분 48초**에 release build 완료를 기록하며 receipt는 `1795336233770201927-5`다. 보관 바이너리 SHA256은 **`72ee31e173eb559e3c454bcae604f742812b88844ae676b6981a935f9ada0798`**다. 비교 출력 v13도 컴파일했고 `quotient16-eight-comparison-v13.json`에서 기존 q16 네 arm의 `validation_passed=true / equivalent=true`와 빈 contract/result differences를 확인했다. 실제 q17 8-lock과 P7P4 결과는 다음 두 절에 기록한다. 이 보관 소스의 branch-bound 기본은 0이며 실제 생략한 후속 solve·kind query만 줄일 수 있고 이미 사용한 producer/physical/kind work를 reset하지 않는다.

### 3.34 q17 실제 8-lock: branch-bound ABBA 완료와 정책 비용 감소

`quotient17-eight-bb0a.json`, `quotient17-eight-bb1a.json`, `quotient17-eight-bb1b.json`, `quotient17-eight-bb0b.json` 순서의 ABBA 네 실행은 같은 q17 source·보관 바이너리·Auto/9-worker·**kick 2·prefix mask 0**을 사용했다. 네 원본 JSON의 metadata에서 group 0, 공개 전 memo 0, edge 2, board·kind·source prefix·boundary·collection·supply multiset·suffix import·source existence·source filter는 모두 1, local A/B는 true임을 확인했다. 실제 N=9·두 projection false이며 branch-bound만 0/1로 바꿨다.

`quotient17-eight-comparison.json`은 **`validation_passed=true / equivalent=true`**, 빈 contract/result differences를 기록한다. 원래 **211,680개 중 211,234개**, 확률 **0.9978930461073318**, coverage SHA256 **`24d3271c4b4af592b97186ce2a0b542c032b4b98c7e93fcb8774a9a687ba9797`**가 모두 같다. 완료·resource non-truncated·BuildUp 0와 branch-bound attempts > 0 gate를 통과했다. 각 JSON의 accepted·board 후보 수도 **2,597개**로 같다. 이는 수치 확인이며 전체 candidate-set digest나 solution identity SHA 검증은 아니다.

| 실행 순서 | branch-bound | 전체 실행 초 | engine 논리 peak bytes | sampled process-group RSS peak bytes |
| --- | ---: | ---: | ---: | ---: |
| bb0a | 0 | 9.985800113 | 326,434,602 | 251,846,656 |
| bb1a | 1 | 3.922695508 | 179,420,990 | 126,533,632 |
| bb1b | 1 | 4.054252699 | 179,424,974 | 215,453,696 |
| bb0b | 0 | 9.41666035 | 326,434,234 | 255,012,864 |

Rust comparator의 off 평균은 **9.7012302315초**, on 평균은 **3.9884741034999998초**, on/off 비율은 **0.411130754380963**다. 각 모드 두 표본의 같은 바이너리 ABBA에서 총시간 감소를 관측했다. 이 완료 fixture의 기술 통계이며 안정적인 개선율·p95 또는 10/11-lock 완료 성능을 추정하지 않는다. 이 측정의 보관 소스는 branch-bound 기본 0이며 각 arm에서 모드를 명시했다.

| 정책 계측 | branch-bound 0 | branch-bound 1 |
| --- | ---: | ---: |
| kind-union work, 두 반복에서 동일 | 22,526,724 | 9,207,914 |
| policy states, 첫 pair | 6,482,316 | 3,376,868 |
| action checks, 첫 pair | 11,754,008 | 5,093,554 |
| bound attempts, 두 반복에서 동일 | 0 | 1,277,413 |
| pruned actions, 두 반복에서 동일 | 0 | 877,540 |
| skipped observations, 두 반복에서 동일 | 0 | 2,445,139 |
| bound checks, 두 반복에서 동일 | 0 | 6,788,135 |

첫 pair의 observation source 준비 **0.088824204 / 0.027212696초**, freeze **0.000487225 / 0.000418862초**, policy **9.787265114 / 3.797355278초**도 기록했다. 이 단일 pair의 producer 시간 차이를 branch-bound가 Geometry를 줄였다는 근거로 사용하지 않는다. 실제 바뀐 작업은 정책의 남은 분기·solve·kind query이며 이전에 발생한 work는 reset하지 않는다. Engine 논리 peak와 sampled RSS는 각각의 계측으로 유지하며 합산하지 않는다.

RSS receipt는 표 순서대로 `1795336549121134468-5`, `1795336559158628502-18`, `1795336563136310560-31`, `1795336567215661941-44`이고 모두 exit 0·kernel peak null이다. P7P4의 group 0/1·branch-bound 1 결과는 다음 절의 미완료 실행이며 원래 10/11-lock 완주 목표는 아직 해소되지 않았다.

### 3.35 q17 실제 P7P4: branch-bound 적용 후에도 정책 work 상한

`quotient17-p7p4-groups0.json/.stderr`와 `quotient17-p7p4-groups1.json/.stderr`는 같은 q17 source·보관 바이너리에서 **branch-bound 1**, Auto/9-worker, N=11·두 projection false를 고정하고 group만 0/1로 실행했다. **Kick 2·prefix mask 0** 및 나머지 고정 모드는 앞의 8-lock ABBA와 같다. Fixture와 공급 길이가 다르고 P7P4가 미완료이므로 두 fixture의 시간을 같은 계약의 A/B로 비교하지 않는다.

두 실행 모두 전체 **456,459개** 후보의 등록·merge·freeze를 끝내고 `board_ready`를 기록했다. 공통 구조는 board **527,568 nodes / 1,683,722 edges**, suffix roots **3,906,552**, prefix visits **15,315,266**, edge attempts **13,160,074**, 중앙 kind nodes **868,853**이다. Source existence는 `[29,856,840, 0, 29,400,381, 456,459, 89,180,737, 33,103,774]`, source filter는 `[456,459, 87,640,128, 52,941,170, 6,379,090]`로 같다. 후보 수와 구조 수치 확인을 전체 candidate-set hash 검증으로 바꾸지 않는다.

| 계측 | groups 0 | groups 1 |
| --- | ---: | ---: |
| 전체 실행 초, 미완료 | 32.340504514 | 34.550092198 |
| observation source 준비 초 | 1.5418135400000001 | 1.446982175 |
| freeze 초 | 0.361761519 | 0.336776957 |
| policy 초, 오류까지 | 2.16434616 | 3.7953646130000003 |
| base physical work | 50,011,805 | 50,077,709 |
| kind-union work | 13,988,195 | 13,922,291 |
| sampled process-group RSS peak bytes | 860,119,040 | 935,497,728 |

각 실행의 base와 kind work 합은 **64,000,000**이다. 최종 JSON은 모두 `complete=false`, `InvalidProblem { reason: "wasm_observation_physical_work_limit" }`이며 실패 지점은 kind-union query다. 같은 producer에서도 worker 할당에 따라 누적 base work가 다르므로 두 arm의 차이를 group index의 순수 절감량으로 해석하지 않는다. 이 실패시간으로 speedup이나 완주 확률을 계산하지 않는다.

로그에서 확인한 정책 progress는 모두 **초기 관측 1/5,040 완료**다. 그 시점의 states/action checks는 groups 0에서 **652,632 / 1,039,592**, groups 1에서 **652,454 / 1,039,293**이다. 이는 해당 progress 시점의 수치이며 최종 정책 전체 수치가 아니다. 둘째 초기 관측의 완료 또는 전체 성공은 로그로 확인되지 않는다. q16의 첫 progress보다 적은 정책 작업이 기록되었지만 별도 소스·실행의 진단이며 이 실패 실행들을 완료 성능 비교로 사용하지 않는다.

| 오류 시 branch-bound 계측 | groups 0 | groups 1 |
| --- | ---: | ---: |
| attempts | 436,895 | 945,131 |
| pruned actions | 213,866 | 476,661 |
| skipped observations | 436,016 | 925,429 |
| checks | 1,676,727 | 3,613,824 |

Groups 1의 group 계측은 `[26,064, 390,487, 2,732,741, 303,634, 0, 0, 390,487, 5,718,600, 0]`이며 이 work는 위 kind work에 이미 포함한다. Kind arena의 오류 시 retained/peak bytes는 groups 0의 **24,065,120 / 27,599,724**, groups 1의 **65,569,597 / 71,406,295**다. 이 arena 범위 수치와 process-group RSS는 서로 다른 계측이며 합산하지 않는다.

Receipt는 groups 0의 `1795336612231813820-5`, groups 1의 `1795336644621073577-18`이다. 둘 다 **child exit 1**, pressure events 0, kernel peak null, supervisor termination reason null을 기록했다. 600초 감독 timeout이나 OOM으로 종료한 실행이 아니다. Branch-bound가 8-lock 정책 비용을 줄였다는 완료 근거와 10-lock의 work 상한 미해결을 함께 유지한다. q17은 producer 구조를 바꾸지 않으므로 §3.28의 관측된 11-lock producer work 하계를 해소했다는 근거도 없다.

### 3.36 q18 H-prefix cone 공유: 구현·검증과 실제 8-lock 비교

소스 `3d460d4a2c1ae44193142ec1762ebd4e1d877378`, 보관 바이너리
`bin/quotient18-observation`, SHA256
`9ae331d323e9216661b4933082fb039ac118ef747e02c43b9569ffc0835a9610`이다.
기본 off인 `CLEARRA_OBSERVATION_PREFIX_CONES`는 같은 원래 H 배치 집합의
**모든 predecessor 경로**가 이미 등록됐다는 worker-local certificate를 공유한다.
Hit에서도 현재 후보의 H suffix root를 추가한다. 정확성은 proof B19의 cone
독립성 정리이며, 고정 65,536-slot cache의 초기화·발행·실제 miss 방문과 메모리를
기존 한도에 포함한다. q17의 branch bound는 이 commit에서 기존 guard를 유지해
제품 기본 on으로 채택했다.

최초 focused 실행에서는 대체 타일링 fixture가 실제 source witness를 갖지 않아
6개 중 1개가 실패했다. 원래 셀을 유지하는 실제 S+T 재타일링으로 fixture를
고친 뒤 **관측 141개, 0 failed, 5.16초**를 통과했다
(`quotient18-observation-tests.log`, receipt `1795338172603621779-5`).
서로 다른 future suffix·충돌·원본 weighted IDs·merge 수명·취소·work cap 등을
확인하는 여섯 새 회귀를 포함한다. Release build는 **1분 43초**,
receipt `1795338242601270391-7`이다. 유한 회귀와 전칭 증명을 구분한다.

ABBA는 kick 2·prefix mask 0·groups 0·branch bound 1, board/kind/source/boundary/
collect/supply/import/existence/filter 1, edge 2·pre-reveal memo 0을 고정하고
prefix cones만 0,1,1,0으로 바꿨다. 네 파일은
`quotient18-eight-cones{0a,1a,1b,0b}.json`이다.

| 순서 | cones | 전체 초 | engine 논리 peak bytes | sampled RSS peak bytes |
| --- | ---: | ---: | ---: | ---: |
| 0a | 0 | 4.339916213 | 179,422,474 | 216,158,208 |
| 1a | 1 | 4.206991842 | 183,628,542 | 214,802,432 |
| 1b | 1 | 3.895278657 | 183,640,934 | 127,479,808 |
| 0b | 0 | 3.677789304 | 179,448,918 | 132,038,656 |

Rust 비교기 `quotient18-eight-comparison.json`은 v15 입력의
**validation=true / equivalent=true**다. 원래 **211,234 / 211,680**, 확률
**0.9978930461073318**, coverage SHA256
`24d3271c4b4af592b97186ce2a0b542c032b4b98c7e93fcb8774a9a687ba9797`가 동일하다.
평균은 **4.0088527585 → 4.0511352495초**, 비율 **1.0105472796201727**로,
이 작은 fixture에서 총시간 개선은 관측되지 않았다. Cache 초기화 비용도
포함한 실제 결과이므로 prefix 방문 감소를 곧바로 시간 개선이라고 쓰지 않는다.

첫 pair의 prefix 방문은 **32,139 → 26,978**, typed edge 등록 시도는
**33,738 → 9,391**이지만 base work는 **124,587 → 690,291**이다.
아홉 worker의 cache 초기화가 총 589,824 work를 추가한다. 두 on 반복의
cone 계측 `[lookups,hits,misses,replacements,publishes]`는 각각
`[19746,14520,5226,36,5226]`, `[19746,14622,5124,38,5124]`이며 worker 작업
분배 차이가 있다. 전역 nodes **1,954**, edges **3,679**, roots **18,941**와
kind work **9,207,914**는 그대로다. Candidate 수 일치는 전체 candidate-set
digest를 계산했다는 뜻이 아니다. 기본 off를 유지한다.

표 순서 receipt는 `1795338425031060500-5`, `1795338429447752394-18`,
`1795338433674784396-31`, `1795338437595611399-44`다. 모두 exit 0이고
kernel memory peak와 supervisor termination reason은 null이다.

### 3.37 q18 실제 P7P4: 생성 작업 감소와 남은 정책 한도

같은 q18 바이너리·위 환경에서 cones=1, `p7p4 visible-7`을 실행했다.
**456,459개 후보 등록·병합·freeze** 뒤 정책 kind union에서
`wasm_observation_physical_work_limit`으로 미완료다. 전체 경과는
**34.376783611초**이며 완료 시간 비교나 speedup으로 사용하지 않는다.
Receipt `1795338441315509744-57`는 child exit 1, sampled RSS
**929,099,776 bytes**, kernel peak null, supervisor reason null이다.

전역 nodes **527,568**, edges **1,683,722**, roots **3,906,552**는 q17과 같다.
실제 등록 prefix 방문은 **9,445,818**, typed edge 시도는 **3,267,658**이다.
정책 시작 전 base work **35,887,800**과 kind work **28,112,200**의 합이
기존 **64,000,000**에 도달했다. q17 groups=0의 base **50,011,805**보다
낮아졌지만 남은 여유가 전체 정책 완주에는 충분하지 않았다. 서로 다른 source
commit의 실패 경과시간으로 개선율을 계산하지 않는다.

Cone 계측은 `[4567118,3546030,1021088,567998,1021088]`이다. 기존 source
필터의 `calls/mask_words/domain_words/prefix_checks`는
`[456459,87640128,52941170,6379090]`이다. Source trie 구성은
**1.231758681초**, freeze는 **0.359458184초**이며 source trie 수치를 전체
Geometry 생성 시간으로 해석하지 않는다. 초기 관측은 첫 **1/5,040**의 완료
로그까지만 확인되며, 다음 256 milestone이 없다는 사실만으로 정확히 하나만
완료됐다고 단정하지 않는다. 전체 coverage나 실패 확률 0을 보고하지 않는다.

실제 11-lock은 달라진 producer 작업량을 확인하기 위해 한 번 재실행했다.
최종 기록은 바로 다음 §3.38에 둔다.

### 3.38 q18 실제 11-lock: 완료된 producer 작업량과 중앙 node 한도

q18의 같은 frozen binary와 §3.37의 고정 옵션으로 실행했다. 원래 12-draw
공급, VisibleSeven, Auto=9, 기존 node/edge/member/work 및 supervisor 한도를
그대로 사용했다. 실행 JSON은 `quotient18-eleven-cones1.json`, stderr는 같은
stem이며 receipt는 `1795338475726672235-70-runtime.json`이다.

결과는 **563.427042523초 후 coordinator의
`wasm_observation_physical_node_limit`**, child exit 1이다. 아홉 worker 모두
종료했고 geometry candidate 414,005,053개 중 원래 source와 양성인 후보
2,008,622개를 등록했다. 세 subset import 기록 뒤 node 한도에 도달했으며
board-ready/freeze 완료와 정책 실행은 확인되지 않았다.
관측 aggregate process-tree peak는 1,535,758,336B, kernel memory peak는 null,
supervisor termination reason도 null이다. OOM이나 600초 timeout으로 기록하지 않는다.

새 Rust 추출기 v2는 모든 worker의 최종 counter와 source partition을 대조했다.
`quotient18-eleven-work-lower-bound.json`의 다음 값은 **worker별 합계**이며,
전역 고유 node/edge/root 수를 뜻하지 않는다.

| 완료된 worker 집계 | 값 |
| --- | ---: |
| 후보 수 | 2,008,622 |
| node / edge / suffix root 합계 | 5,454,607 / 9,569,274 / 12,980,663 |
| prefix 방문 / edge 등록 시도 | 50,052,910 / 14,181,158 |
| 이미 수행한 work | **71,396,217** |
| cone 조회 / hit / miss | 14,751,606 / 10,187,903 / 4,563,703 |
| cone 교체 / 완료 publish | 3,976,413 / 4,563,703 |

이 합계만으로 기존 누적 64M work 한도를 넘는다. 병합은 각 source work를
한 번씩 포함하고 추가 작업도 더하므로, q18 producer를 고정한 중앙 node 통합이나
정책 변경만으로는 11-lock을 완주할 수 없다. 이는 이번 producer와 실행의 하한이며
다른 알고리즘의 불가능성 증명이 아니다. q15의 **중간 milestone 하한** 77,863,423과
q18의 **완료된 worker 합계** 71,396,217은 서로 다른 집계 시점이므로 완전한
producer 감소율로 나누지 않는다. 다음 실험에는 producer 재탐색 자체를 줄이는
구조 변경이 필요하고, 동일 구조의 11-lock 재실행은 생략한다.

### 3.39 q19/q20 통합 검증

q20 helper 원본 commit은 `051f87ebec39c6875839c09274f4780c234fd7dc`이며,
primary에는 `f65bdf6`으로 반영했다. q20 evaluator 연결은 `f2ef5b3`,
q19의 완료된 boundary-class batch와 q20 board-view 연결은 `13d539b`다.
q19 helper는 동일한 완성 root 집합과 remaining으로만 class를 만들고, q20은 실제
관련 draw가 전부 관측된 H에서만 exact word membership을 요청한다.

| 검증 | 결과 | Receipt |
| --- | --- | --- |
| isolated q20 helper의 `word_bitmap_` | 9 passed, 0 failed, 0.24초; compile 58.84초 | `1795339393387890748-5` |
| primary q19/q20 통합 `observation` | 161 passed, 0 failed, 5.68초; compile 49.07초 | `1795339839900050976-5` |

두 실행은 `parallel,local-search-ab,placement-dependency-pc`, 단일 test thread,
기존 build-test supervision을 사용했다. 로그는 `quotient20-word-bitmap-tests.log`,
`quotient20-observation-tests.log`다. 새로운 통합 회귀에는 q19 5개, q20 helper 9개,
q20 evaluator 6개가 포함된다. 이 유한 검증은 B20/B21 전칭 증명과 구분한다.
실제 성능·10/11-lock 완주를 아직 이 결과로 주장하지 않는다.

후속 비교기는 v16의 두 새 mode와 17축을 받는 출력 v15로 확장했다.
컴파일 뒤 q18의 실제 네 arm을 다시 입력한
`quotient18-eight-comparison-v15.json`은 validation/equivalent 모두 true다.
v16의 실제 benchmark 검증은 별도로 기록한다.

### 3.40 q21 통합 검증과 계측 계약

Primary 코드 commit은 **`2d99de404e567d7e83eaed321d447bcc3e66a9cd`**다.
q19의 reuse 집합 정렬 생략 `d6df081`과 q20, q21을 함께 포함한다.
`quotient21-observation-tests.log`는 **166 passed / 0 failed / 6.38초**이며,
receipt는 `1795340558581647068-5`다. q20의 161개와 중복 합산하지 않는다.
새 q21 4개와 q19 class-collision 1개 회귀를 추가했고 기존 weighted-ID 회귀도
mode 2까지 확장했다.

`PREFIX_CONES=0/1/2`는 각각 기존 등록, 작은 H direct cache, H와 중간 T의
bounded exact cache다. 기본 0은 유지한다. Exact cache는 최대 1,000,000개
certificate payload slots와 512K개의 u32 bucket slots를 사용한다.
Payload 최대 64,000,000B, bucket 최대 2,097,152B이며 chunk metadata·scratch·
direct fallback이 실제로 사용되면 그 공간도 포함한다. 기존 graph node/edge/member/
누적 work 한도를 바꾸지 않았고, optional cache의 cap·할당 거절은 기존 계산으로 돌아간다.

기존 H 계측 5개는 뜻을 유지한다. 새 exact 계측 9개의 entries/capacity는
**worker별 최종 producer cache 스냅샷 합계**이며 중앙의 현재 cache 크기가 아니다.
나머지는 ancestor lookups/hits, full fallbacks, grows, rehashed entries,
initialized slots, work다. Grows는 새 chunk 할당과 bucket 성장의 횟수이며,
initialized slots는 u32 bucket 초기화 수다. 마지막 work는 기존 board work에
이미 포함된 부분집합이다. 같은 작업을 두 번 더하지 않는다.

독립 Rust 작업량 추출기도 v3으로 확장하여 worker별 exact 계측을 검증하고
그 합을 별도 필드로 기록한다. q15 v1·q18 v2의 기존 결과 파일은 보존한다.
실제 실행 결과와 동일 binary의 단일 축 비교는 아래에 이어서 기록한다.

### 3.41 q21 실제 8-lock: exact cone와 boundary class의 별도 ABBA

보관 binary `bin/quotient21-observation`의 SHA-256은
`f1578b9a749461c18ec5b27a256a708957ad48099ed0a56c75ff6895f76e5b3d`다.
§3.40의 전체 source SHA로 release build를 완료했으며 build receipt는
`1795340655944876821-5`, compile 시간은 1분 50초다. 입력 schema는 v16이다.
두 ABBA block은 모두 kick 2, prefix mask 0, branch-bound 1, word bitmap 0과
자동 9 workers를 사용했다. 나머지 source·board·kind·boundary suffix·edge closure·
collection·multiset·subset import 설정은 §7.16과 같다.

| 별도 block | Arm 순서 | 완료 시간, 초 | 평균 reference → treatment | 평균 비율 |
| --- | --- | --- | --- | --- |
| exact cone | cone 1a, 2a, 2b, 1b; class 0 고정 | 4.373000099, 3.908395962, 3.981088648, 3.637040765 | 4.005020432 → 3.944742305 | 0.9849493584306388 |
| boundary class | class 0a, 1a, 1b, 0b; cone 2 고정 | 4.682269203, 3.665768831, 3.901271377, 3.580189224 | 4.1312292135 → 3.783520104 | 0.9158339826887943 |

`quotient21-eight-cone-comparison.json`과
`quotient21-eight-class-comparison.json`은 각각 네 arm의 validation/equivalent가
모두 true다. 여덟 실행 모두 원래 공급 **211,234 / 211,680**, 확률
**0.9978930461073318**, coverage SHA
`24d3271c4b4af592b97186ce2a0b542c032b4b98c7e93fcb8774a9a687ba9797`다.
서로 다른 시간대의 두 reference 집합을 하나의 평균으로 합치지 않는다.
각 모드 두 표본의 기술 통계이며 일반적 반복성이나 유의성 주장은 아니다.

Cone block의 node/edge/root는 1,954 / 3,679 / 18,941이고 kind work는
9,207,914로 동일하다. Producer work의 평균은 633,669에서 54,885.5로 줄었지만,
감소분의 약 **99.73%는 cache 초기화·관리 비용 감소**다. 실제 candidate·prefix·edge
등록 작업은 38,704에서 37,117.5로 약 4.10% 줄었다. 작은 fixture의 총 producer work
감소율을 11-lock의 실제 경로 방문 감소율로 일반화하지 않는다.
첫 mode-2 arm의 exact 계측은
`[5567,36864,5182,4561,0,18,0,9216,17621]`이다.

Class block의 첫 mode-1 arm은 node 1,930, edge 3,679, root 18,872,
kind work 9,205,521이다. Boundary 계측은
`[4963,1850,1826,24,18872,7527520,295696]`이며 1,850개 고유 H 보드에서
정확히 같은 root 집합의 재사용은 24회다. 첫 arm engine peak는 class 0의
179,724,198B에서 class 1의 186,734,482B로 늘었다. 시간과 공간 효과를 구분한다.

Cone block receipt는 순서대로 `1795340800848655045-5`,
`1795340805271286606-18`, `1795340809210615789-31`,
`1795340813234670755-44`다. Class block은 `1795341040838788851-5`,
`1795341045561757580-18`, `1795341049275811274-31`,
`1795341053199374936-44`다. 해당 JSON·stderr는 각각
`quotient21-eight-cone{1a,2a,2b,1b}`와
`quotient21-eight-class{0a,1a,1b,0b}`로 보존했다.

### 3.42 q21 실제 8-lock: 첫 word bitmap 구현은 work 한도에서 실패

같은 binary의 cone 2·class 0에서 word bitmap 0 기준 arm은 3.677695406초에
완료했다. 이어진 bitmap 1은 **1.934225535초에 64M work 한도로 미완료**다.
`quotient21-eight-bitmap1a.json`과 stderr의 정확한 정책 진행량은
**607 / 5,040 초기 관측**이며, base 111,486 + kind 63,888,514 = 64,000,000이다.
실패 시간을 완료 시간과 비교한 속도 개선율을 만들지 않는다.

기존 bitmap 9계측은
`[779330,728903,50426,38525464,13306,1756366,3050958,0,63888514]`다.
Query 생성 50,426회마다 764 step을 사용해 **38,525,464 query steps**가 발생했다.
이는 실패 시 kind work의 약 60.3%다. 전체 raw query별로 가능한 모든 multiset과
Hold 경로를 구성하는 비용이 반복됐다. 뒤의 q22는 실제 target multiset만 구성하고,
kind의 순서를 보존하는 재명명으로 cache를 공유하며, frozen board의 root OR를
한 번만 만드는 방향으로 이 반복을 제거한다.

Receipt `1795340820610555739-70`은 child exit 1, duration 1,954ms,
supervisor termination reason null이다. 저빈도 RSS sample 77,824B는 이 짧은
실행의 실제 메모리 사용량을 대표하지 않으며 메모리 개선 근거로 사용하지 않는다.
실패가 확인된 뒤 같은 bitmap 구현으로 예정됐던 나머지 ABBA 및 대형 실행은
반복하지 않았다. 정확성 증명의 성립과 자원 한도 내 완주는 별도 요건이다.

### 3.43 q21 실제 10-lock: cone의 cache 성장과 boundary batch 비용

P7P4는 cone 2·word bitmap 0에서 class 0과 1을 각각 실행했다. 원래 공급과
자동 9 workers, 모든 graph/member/work 및 감독 자원 한도를 유지했다.

| 항목 | Class 0 | Class 1 |
| --- | --- | --- |
| 종료시간 | 33.968020478초 | 28.379285690초 |
| 최종 상태 | freeze 완료 후 policy work limit | boundary batch work limit, freeze 전 |
| 완료된 source 후보 | 456,459 | 456,459, 모든 worker 생산 종료 |
| 정책 진행 | 정확히 4 / 5,040 | 정책에 진입하지 않음 |
| Base / 추가 kind work | 39,501,746 / 24,498,254 | 완료되지 않은 batch snapshot |
| 감독 sampled RSS peak | 946,065,408B | 638,857,216B |
| Receipt | `1795341056818865954-57` | `1795341090868932503-70` |

두 receipt 모두 child exit 1, kernel peak와 supervisor termination reason은
null이다. 실패 단계가 다른 두 종료시간을 성능 우열이나 정상 확률 0으로 해석하지 않는다.

Class 0의 최종 graph는 527,568 nodes, 1,683,722 edges, 3,906,552 roots다.
모든 worker의 종료 계측을 Rust v3 추출기로 합산한
`quotient21-p7p4-work-bound.json`에서 producer work는 **18,043,465**다.
q18의 같은 공급·자원 실행에서는 **14,437,368**이었다.
실제 candidate·prefix·edge 등록은 **12,826,456 → 9,894,564**로 줄었지만,
cache 비용이 **1,610,912 → 8,148,901**로 늘어 전체 producer 비용이 악화됐다.
각 worker의 큰 H key 때문에 512K bucket으로 성장하면서 초기화 5,391,360회와
재배치 672,768회가 발생했다. Exact entries 1,299,073 중 H는 885,411,
중간 T는 413,662이며 worker별 T 최대는 61,364였다. q22의 mode 3은 H를 작은
기존 direct cache에 두고 T에만 exact map을 사용해 이 실제 성장 비용을 피한다.

Class 1은 H 원본 709,598개를 402,256개 고유 보드로 모은 뒤 class를 처리하던
중에 64M에 도달했다. 부분 boundary 계측은
`[709598,402256,191345,86055,2601150,632625832,45964543]`이다.
Class 191,345 + reuse 86,055 = 277,400개 보드만 처리된 상태이며 최종 class 수가
아니다. Batch 추가 work 45,964,543과 worker source work 18,035,457이 한도에
합산됐다. 당시 engine retained/peak는 71,730,592B / 725,252,424B다.

q21에서 확인한 cache 성장·batch 병합 비용을 줄이는 새 구현 없이 11-lock을
되풀이하지 않았다. q18의 shard 수치를 q21에서도 반드시 같다고 가정한 비용식을
보편 하한으로 사용하지 않으며, 위 실제 10-lock 측정과 별도의 q18 11-lock
완료 producer 기록을 다음 구현의 판단 근거로 삼는다.

### 3.44 q22 통합 검증과 고정 바이너리

q22 source는 **`8f63f0cdbd1323de6eb454f4e4bc9b08accdc25f`**다.
경계의 typed provisional remap·완료 unordered roots·H direct/T exact mode 3은
`d9e9d7a`, target-M canonical query·raw profile·board OR·Full/Empty certificate는
`8f63f0c`에 통합했다. 증명은 B19의 mode 3, B20, B21a/B21b를 따른다.

관측 focused 검증은 **176 passed, 0 failed, 5.39초**, compile 43.71초이며
receipt는 `1795342846083143178-5`다. 기존 166개와 중복 합산하지 않는다.
Release는 **1분 44초**, receipt `1795342915539168508-5`이고 warning 없이 끝났다.
`bin/quotient22-observation`의 SHA-256은
`9cf6702d030d27b463f780f05a7b67f4e4c6bdea62e26d517f80727519402197`다.
입력 schema는 v17이다. 아래 측정 중 Cargo/rustc를 실행하지 않았고 자동 9 workers,
graph/member/work 및 감독 한도를 유지했다.

### 3.45 q22 실제 8-lock: 선택적 cone은 완료, bitmap은 완료하지만 더 비쌈

두 block 모두 boundary classes 0이며 같은 고정 바이너리를 사용했다.
Cone block은 bitmap 0, bitmap block은 cone 3을 고정했다.

| 별도 block | Arm 순서 | 완료 시간, 초 | 평균 reference → treatment | 평균 비율 |
| --- | --- | --- | --- | --- |
| Cone | 2a, 3a, 3b, 2b | 4.288788191, 3.964476163, 3.698885584, 3.658795291 | 3.973791741 → 3.8316808735 | 0.9642379679755844 |
| Bitmap | 0a, 1a, 1b, 0b | 3.662890473, 7.260023609, 6.999019397, 3.986168835 | 3.824529654 → 7.129521503 | 1.864156418696708 |

`quotient22-eight-cone-comparison.json`과
`quotient22-eight-bitmap-comparison.json`은 validation/equivalent 모두 true다.
여덟 실행 모두 원래 **211,234 / 211,680**, 확률 **0.9978930461073318**과
coverage SHA `24d3271c4b4af592b97186ce2a0b542c032b4b98c7e93fcb8774a9a687ba9797`가
같다. q21의 bitmap 미완료를 해결했지만, 현재 fixture에서는 bitmap의 완료 비용이
오히려 커졌다. Bitmap의 제품 기본은 계속 0이다. Cone 3의 작은 시간 차이도
각 두 표본의 기술 통계이며 안정적인 개선율로 단정하지 않는다.

작은 8-lock에서 cone 3은 H direct cache의 초기화 비용을 여전히 낸다.
첫 arm board work는 cone 2의 112,475에서 cone 3의 699,326으로 증가했으며,
kind work 9,207,914는 같았다. Mode 3의 목적은 큰 H 집합으로 exact map이
성장하는 비용을 피하는 것이고, 모든 규모에서 work가 줄어든다는 보증이 아니다.

Bitmap 1의 두 arm은 kind work **48,514,384**로, off의 **9,207,914**보다 컸다.
그 안의 bitmap work는 **42,010,338**, board cache work는 **11,889,946**이다.
이 세 수는 포함관계이므로 합산하지 않는다. Membership 6,885,320회 중 fallback이
5,122,535회 발생했다. Raw profile은 65,536개 상한을 공유하며, target query builds는
41,188, target cache hits 668,065, query steps 24,830,864였다. Board queries
8,870,950, builds 100,210, hits 8,770,740, board fallbacks 0, retained payload를
새로 게시한 누적 word 수 38,604를 기록했다. Bitmap 전체 게시 word 수 447,135에는
root/query/board payload가 모두 포함된다. 반복 lookup과 profile 상한 뒤의 기존 DP
재진입 비용이 전체 성능 판단에서 중요하다.

Bitmap block 첫 off/on engine peak는 **183,950,274B → 246,340,626B**,
감독 sampled RSS peak는 **126,889,984B → 216,068,096B**였다.
두 번째 on/off RSS는 220,499,968B / 128,536,576B다. 메모리도 감소하지 않았다.
Cone block의 RSS는 순서대로 216,829,952B / 128,659,456B / 124,633,088B /
128,344,064B이며 첫 실행과 이후 실행의 차이를 숨기지 않는다.

Cone receipt는 순서대로 `1795343044517851120-6`, `1795343048836936732-20`,
`1795343052858487069-34`, `1795343056573741061-48`이다.
Bitmap receipt는 `1795343060298738609-62`, `1795343064013616710-76`,
`1795343071308590482-90`, `1795343078341874653-103`이다.
모든 receipt의 child exit는 0이며 kernel peak/감독 종료 원인은 null이다.

### 3.46 q22 실제 10-lock: 경계 병합 완료와 남은 정책 work

같은 binary의 cone 3에서 class 0·bitmap 0, class 1·bitmap 0,
class 1·bitmap 1을 순서대로 실행했다. 세 실행 모두 **456,459개 후보의 생산,
중앙 graph와 freeze를 완료**했지만 정책 단계의 기존 64M work 상한에서 끝났다.

| 항목 | Class 0 / bitmap 0 | Class 1 / bitmap 0 | Class 1 / bitmap 1 |
| --- | ---: | ---: | ---: |
| 종료시간, 초 | 36.229423267 | 35.914825354 | 36.853790565 |
| Nodes | 527,568 | 374,840 | 374,840 |
| Edges | 1,683,722 | 1,683,722 | 1,683,722 |
| Suffix roots | 3,906,552 | 3,227,517 | 3,227,517 |
| Policy 전 base work | 34,951,050 | 45,586,680 | 45,581,685 |
| 추가 kind work | 29,048,950 | 18,413,320 | 18,418,314 |
| 완료 초기 관측 / 5,040 | 6 | 4 | 0 |
| 감독 sampled RSS peak | 1,004,257,280B | 876,527,616B | 845,037,568B |

마지막 실행은 다음 작업 청구를 수용하지 못해 합계 63,999,999에서 거절됐다.
상한을 낮추거나 1 unit을 정상 완료로 해석하지 않는다. 세 실패 종료시간으로
성능 개선율을 만들지 않는다. 원본 파일은 각각
`quotient22-p7p4-cones3-class0-bitmap0`,
`quotient22-p7p4-cones3-class1-bitmap0`,
`quotient22-p7p4-cones3-class1-bitmap1`의 JSON·stderr다.
Receipt는 같은 순서로 `1795343138481858263-6`, `1795343174775184329-20`,
`1795343210767845905-33`이며 child exit 1, kernel peak와 감독 종료 원인은 null이다.

Class 0의 worker 종료 계측을 Rust v3 추출기로 합산한
`quotient22-p7p4-work-bound.json`에서 producer work는 **13,635,532**다.
q21 cone 2 실행의 18,043,465보다 줄었고, exact cache work도
8,148,901에서 **1,596,430**으로 줄었다. q22의 exact entries/capacity는
394,065 / 417,792, initialized slots 672,768, rehashed 82,944다.
Worker별 입력 분배와 H direct miss는 실행마다 달라질 수 있으므로 이 계측 차이를
시간 ABBA나 고정 shard 수학식으로 해석하지 않는다.

Class 1의 bitmap 0은 402,256개 고유 H 보드를 **249,528개 class**로 만들었고,
재사용은 **152,728회**다. Boundary 추가 work는 **18,742,405**이며 q21의 정렬·
remap 비용 때문에 중간에 멈춘 batch를 이제 끝까지 처리했다. 실제 H 원본 698,546개와
staging peak 652,097,968B는 별도 계측이다. 완성 후 graph 축소와 전체 work 증가는
동시에 성립하므로 저장 공간 감소만으로 정책 완주를 주장하지 않는다.

Bitmap 1의 policy work 18,418,314 중 board OR/cache work가 15,300,418이다.
Query build는 2,641회, query steps 1,816,423이고 membership 592,534회에서
fallback 0이었다. 따라서 이 10-lock 실패를 8-lock의 raw profile cap 문제와 같은
원인으로 단정하지 않는다. Board queries 3,183,535 중 342,918회 build가 발생했고,
여기서는 root OR의 첫 구성과 반복 조회가 지배적이었다.

다음 q23은 9개 worker의 후보 생산은 유지하면서 physical 등록과 destination kind
owner를 요청 단위로 공유한다. 같은 producer를 더 오래 실행하거나 resource 한도를
바꾸는 대신, 실제 shard 중복 등록과 최종 physical 병합을 제거하는 구조다.
11-lock은 이 공유 경로를 통합한 뒤 실행하며 q22만의 11-lock 결과는 기록하지 않는다.

Class 축소 뒤 기존 q16 multiset group index의 비용이 달라지는지 확인하려고
같은 q22 binary의 class 1·cone 3·bitmap 0에서 **groups만 1**로 바꾼 대조를
추가했다. `quotient22-p7p4-cones3-class1-groups1`은 **36.124355522초** 후
같은 work 상한으로 미완료였다. Base work 45,618,749, 완료 초기 관측은
**10 / 5,040**으로 group 0 실행의 4개보다 늘었다. 같은 시간 완료 결과나 속도
개선율로 표현하지 않는다. Graph의 374,840 nodes / 1,683,722 edges /
3,227,517 roots는 같았다.

Group 계측은 `[27673,450534,2843929,344431,0,0,450534,6217905,0]`이다.
Build root visits 450,534와 matched visits 344,431을 기록했고 fallback/eviction은
없었다. 원래 root 반복 검사의 감소와 group 구성·조회 비용 6,217,905를 함께
해석한다. Uniform certificate는 288,732 nodes / 559,516 edges였으므로,
certificate 전체를 미리 만드는 후속 후보가 줄일 수 있는 비용과 반복 root
filter 비용도 구분해야 한다. Receipt `1795344042789443482-5`는 child exit 1,
duration 36,183ms, sampled RSS 974,569,472B, kernel peak/감독 종료 원인 null이다.

### 3.47 q23 공유 생산자 통합과 검증

Native wiring은 `e50c2f1`, worker별 증분 kind importer는 `3fecfec`, 최초 원인
진단 연결은 `7048fe4`, result/metrics 연결은 `5e379d0`, 공유 pending-H 커널은
`275e07d`로 통합했다. 공유 graph/kind owner는 한 번만 보유하고 worker별 물리·
공급 컴파일과 private source/map은 유지한다. 등록만 같은 owner에 수행하며,
모든 canonical branch 완료 뒤에만 H group을 확정한다. 정확성은 B22를 따른다.

첫 통합 테스트 compile에서 nested test module이 `PieceLanguageIncrementalImport`를
잘못된 부모 module에서 import한 오류를 수정했다. 첫 실패 receipt는
`1795344427424949424-5`이며 성공 기록으로 합산하지 않는다.
수정 후 관측 검증은 **179 passed / 0 failed / 5.71초**, compile 47.08초,
receipt **`1795344478508801497-5`**다. 추가된 shared 회귀 세 개는 실제 두 worker의
다른 arena와 반대 등록 순서를 기존 completed-shard 경로와 비교하며 union 0/1의
physical paths·원본 IDs·가중치·종료 깊이를 확인한다. 한도 회귀는 counter 경계
검증이며 8M 원소 전체 benchmark로 표현하지 않는다.

별도 `incremental_import` 회귀는 **5 passed / 0 failed / 0.00초**, compile 0.11초,
receipt **`1795344663487136899-5`**다. Same-kind NFA 보존·append 후 hit·source epoch
reset·destination identity·취소/부분 실패의 5개 회귀를 포함한다. 실행 전에 source
read-only peer 검토도 완료했지만, 그 검토와 유한 회귀를 전칭 증명으로 대체하지 않는다.
Default/no-PC에서 사용되지 않는 getter cfg와 reexport 경계도 정리했다.

위 두 필터의 서로 다른 검증은 총 **184개**이며, 관측 필터 자체를 184개로
표시하지 않는다. 고정 source는 **`055427a8381a25e38646013b57eb150758b44b8e`**,
release는 **1분 50초**, receipt **`1795344715295821966-5`**, child exit 0이다.
보관 `bin/quotient23-observation`의 SHA-256은
`edb5a6f789143c087a79fd81eb05b2abc4668850a7009f36f8dd34f251ec73fd`다.

후속 비교기는 입력 v18/출력 v17/18축으로 확장했다. 실제 q22 bitmap ABBA를 다시
읽은 `quotient22-eight-bitmap-comparison-v17.json`은 validation/equivalent 모두 true다.
실제 v18 shared ABBA의 완료 결과는 다음 절에 기록한다. 10-lock은 그다음 절의
미완료이고, 11-lock은 이 문서 갱신 시점에 실행 중이다.

### 3.48 q23 실제 8-lock: 공유 생산자 ABBA 완료

같은 q23 바이너리의 `eight visible-7`에서 shared를 **0a, 1a, 1b, 0b** 순서로
실행했다. 네 arm 모두 groups 1·cone 3·boundary classes 1·bitmap 0·kick 2·
prefix mask 0·branch-bound 1이며, 나머지 축도 §7.18의 값으로 고정했다.
자동 9 workers와 원래 9-draw 공급, 두 projection=false를 유지했다.

| Arm | 실행시간, 초 | Engine CPU peak, B | 감독 sampled RSS peak, B | Receipt |
| --- | ---: | ---: | ---: | --- |
| Shared 0a | 4.210744704 | 225,943,115 | 249,503,744 | `1795345040560004429-8` |
| Shared 1a | 3.880813512 | 185,808,107 | 131,825,664 | `1795345044826552491-22` |
| Shared 1b | 3.940469812 | 185,807,879 | 129,671,168 | `1795345048752977603-36` |
| Shared 0b | 3.840998097 | 225,672,287 | 160,133,120 | `1795345052723794761-49` |

`quotient23-eight-shared-comparison.json`의 validation/equivalent는 모두 true다.
Rust comparator의 평균은 **4.0258714005초 → 3.9106416619999997초**, 비율은
**0.971377690185114**, 표본은 각 두 개다. 이 block에서 평균 시간과 engine peak가
낮았다는 관측이며 안정적인 개선율·p95를 주장하지 않는다. 첫 off의 RSS와 시간이
두 번째 off보다 높았다는 차이도 표에서 그대로 보존한다. 모든 receipt는 child
exit 0, pressure events 0, kernel peak/감독 종료 원인 null이다. Engine의 논리적
payload peak와 감독의 sampled process-group RSS는 서로 다른 측정량이다.

네 실행 모두 원래 **211,234 / 211,680**, 확률 **0.9978930461073318**,
coverage SHA **`24d3271c4b4af592b97186ce2a0b542c032b4b98c7e93fcb8774a9a687ba9797`**다.
원래 coverage·probability·packing의 완료 gate와 BuildUp 0도 유지했다.
Geometry 후보 27,987개, accepted/board 후보 2,597개, 최종 1,930 nodes /
3,679 edges / 18,872 roots는 같았다. Candidate-set digest는 미계산이므로 후보
개수 확인을 전체 후보 집합의 해시 동등성이나 전칭 증명으로 표현하지 않는다.

Shared 1a의 request snapshot은 registrations **2,597**, lock entries **2,598**,
pending groups **1,850**, pending root references **18,941**, final classes **1,826**,
finalization work **64,296**, retained **1,554,240B**, peak **7,458,548B**다.
이 값은 공유 request 한 개의 값이며 worker snapshot 아홉 개를 합산하지 않는다.
Finalization work는 board work의 부분량이다. 첫 off/on의 board work는
**752,023 → 186,459**, kind-union work는 둘 다 **8,664,991**이었다.
두 번째 on/off의 board work는 186,472 / 752,029로 worker 입력 분배의 작은 차이가
남는다. 실제 policy states와 action checks에도 작은 차이가 있지만 comparator는
표현·평가 진단값과 원래 coverage·확률 계약을 구분한다.

첫 off/on의 source 단계는 0.074962283 / 0.034808557초, freeze는
0.000265921 / 0.0003779초, policy는 4.010217763 / 3.730627271초였다.
공유 등록의 실제 작업·보유량 감소를 기록하되, 작은 전체시간 차이를 source 단계
단독의 인과 효과로 단정하지 않는다. 원본은 `quotient23-eight-shared{0a,1a,1b,0b}`의
JSON·stderr와 위 receipt다.

### 3.49 q23 실제 10-lock: 공유 등록 완료 후 정책 work 상한

`p7p4 visible-7`에서 shared 1을 고정하고 groups 0, 1 순서로 실행했다.
두 실행 모두 **456,459개** 후보의 생산·공유 등록·class 확정·freeze를 마쳤으나
정책 kind-union work가 기존 합계 **64,000,000**에 도달해 거절됐다.
완료된 확률·coverage 결과는 없으며 두 실패 종료시간의 비율을 계산하지 않는다.

| 항목 | Groups 0 | Groups 1 |
| --- | ---: | ---: |
| 실패 종료시간, 초 | 34.711053326 | 39.884379112 |
| Policy 전 base work | 41,417,099 | 41,437,597 |
| 추가 kind work | 22,582,901 | 22,562,403 |
| Policy 시간, 초 | 3.661776938 | 8.117402269 |
| 완료 초기 관측 / 5,040 | 4 | 13 |
| 감독 sampled RSS peak, B | 807,550,976 | 919,941,120 |
| Receipt | `1795345070698631191-6` | `1795345105458313661-19` |

최종 graph는 두 실행 모두 **374,840 nodes / 1,683,722 edges / 3,227,517 roots**다.
고유 H 보드 **402,256개**를 **249,528개 class**로 만들었고 재사용은 **152,728회**,
finalization work는 **14,812,983**이었다. Source 단계는 group 0/1 각각
1.358445372 / 1.352971373초, freeze는 0.312315557 / 0.287742707초다.
같은 구조에서 group 1의 완료 초기 관측 수가 더 많았지만, 일부 관측의 진척을
전체 성공률이나 완료 성능으로 바꾸어 해석하지 않는다. 입력 분배·cone 재사용 때문에
base work가 소폭 다르며 고정 shard 차감식으로 공유 효과를 계산하지 않는다.

두 receipt는 child exit 1, pressure events 0, kernel peak/감독 종료 원인 null이다.
이는 product의 work-limit 거절이며 600초 timeout 또는 OOM 기록이 아니다.
원본은 `quotient23-p7p4-shared1-groups{0,1}`의 JSON·stderr다.
후속 `quotient23-eleven-shared1-groups1`은 이 갱신 시점에 실행 중이다. 완료 여부와
최종 단계·resource 계측은 최종 artifact가 나온 뒤 별도로 기록한다.

### 3.50 q23 실제 11-lock: 공유 생성 중 suffix 참조 상한

`quotient23-eleven-shared1-groups1`은 같은 q23 바이너리, shared 1·group 1·cone 3·
class 1·bitmap 0, Auto 9 workers 및 기존 자원 한도로 실행했다. **355.943417559초**에
미완료 종료했다. 최초 원인은 공유 등록의 `wasm_observation_physical_union_limit`이며,
peer 중단을 거쳐 공개 최종 오류는 `wasm_observation_shared_producer_failed`다.
최초 원인과 최종 전파 오류를 구분한다.

| 중단 시 진단 | 값 |
| --- | ---: |
| 완료된 후보 등록 | 1,285,619 |
| 실제 pre-H physical nodes | 962,289 |
| 실제 물리 간선 | 4,737,867 |
| Pending H board groups | 1,311,519 |
| 보유 suffix root 참조 | **8,000,000** |
| 누적 physical work | 47,272,836 |
| Prefix 방문 / edge 시도 | 19,929,165 / 10,575,288 |
| 공유 owner 보유 / peak bytes | 727,059,740 / 797,625,924 |
| 감독 sampled RSS | 984,788,992 bytes |

Receipt는 `1795345150022203304-5-runtime.json`, 감독 duration은 356,004ms다.
실행 exit는 1이며 `kernel_memory_peak`와 `supervisor_termination_reason`은 null이다.
이 결과를 OOM, timeout 또는 완료 검색시간으로 해석하지 않는다. H finalization과
정책 평가는 실행하지 못했으며 최종 coverage와 확률은 없다.

이 실행은 q18의 worker별 work 합산과 다른 단일 공유 owner 알고리즘이다. 따라서
q18의 71,396,217 하한을 이 실행에 그대로 대입하거나, 중단 work 47,272,836을 완성된
생산 work로 보고하지 않는다. 확인된 사실은 **완료 전에 현재 root-list 표현이 8M
참조에 도달했다**는 것이다. q24/q25의 후처리·정책 개선은 이 생성 단계의 제한을
그 자체로 해결하지 않는다. 11-lock을 위해서는 생산 중에도 유효한 개별 suffix 언어
동치·포함 증거 또는 더 작은 정확한 표현과 그 work/peak 계약이 추가로 필요하다.

## 4. 유한 microbenchmark: 생성비용과 downstream 작업

근거는 `kick-pruning-micro.jsonl`의 `clearra.global-kick-pruning-cost-summary.v1` 및 `kick-pruning-micro.stdout`이다. 2/4/6L × 5 profile × 7 piece의 105개 template와 1,075개 target query를 사용한다. 독립 primitive reference와의 결과 비교는 모든 arm에서 통과했다.

warmup 3개는 통계에서 제외했다. Off–Current와 Current–Improved를 각각 2회 ABBA 순서로 측정했으며 표본수는 Off 4, Current 8, Improved 4다. 타이머는 template·CSR·query의 전체 batch 바깥에 있고, summary와 binary hashing은 모든 측정 후 수행한다.

### 4.1 Rust summary의 시간

단위는 ns다. 아래 값은 summary의 평균이며, 마지막 두 열은 같은 summary의 template+query 최소·최대다.

| 모드 | n | 전체 template 생성 평균 | reverse CSR 생성 평균 | query 평균 | template+query 평균 | total 최소 | total 최대 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Off | 4 | 3,932,840.25 | 1,346,905.25 | 5,317,952.5 | 9,250,792.75 | 8,982,587 | 9,520,316 |
| Current | 8 | 4,643,173.25 | 1,976,300.125 | 4,596,426.25 | 9,239,599.5 | 8,730,876 | 10,073,613 |
| Improved | 4 | 4,401,762.75 | 1,781,663.5 | 4,416,965.5 | 8,818,728.25 | 8,601,100 | 9,025,389 |

reverse CSR 생성은 전체 template 생성에 포함된 단계를 별도 입력으로 측정한 진단이다. total에 다시 더하지 않는다. 이 벤치의 total은 template 생성과 고정 query workload의 합이며, 실제 검색의 후보 생산·공급 product·결과 조립까지 포함하는 end-to-end 시간은 아니다.

### 4.2 실제 검사량과 공간

아래 카운터는 각 모드의 반복에서 최소·최대가 같은 값이다. compile과 edge 수는 batch에서 각 template을 한 번씩 합산하고 query 수는 실제 query 작업을 합산한다.

| 항목 | Off | Current | Improved |
| --- | ---: | ---: | ---: |
| raw reverse candidate 입력 수 | 240,903 | 240,903 | 240,903 |
| retained reverse edge 수 | 240,903 | 122,101 | 122,101 |
| compile predicate 호출 | 0 | 481,806 | 240,903 |
| compile prior-mask 검사 | 0 | 549,686 | 274,843 |
| query reverse candidate 검사 | 282,352 | 216,215 | 216,215 |
| query original prior guard 검사 | 192,398 | 137,206 | 137,206 |
| query ordered-kick terms | 474,750 | 353,421 | 353,421 |
| query pose domain 초기화 | 83,051 | 66,721 | 66,721 |
| query reached pose-lanes | 71,660 | 71,660 | 71,660 |
| query worklist pops | 40,532 | 40,532 | 40,532 |
| template 한 개의 transient bitmap peak bytes | 0 | 0 | 760 |
| query sampled logical-byte peak | 11,072 | 11,072 | 11,072 |

이 결과는 두 비용을 함께 보여준다. Current는 Off보다 template 생성에 판정 비용을 더 쓰고 query 검사량을 줄인다. 이 고정 workload에서는 두 효과가 total 평균에서 거의 상쇄된다. Improved는 같은 E2 결과를 bitmap으로 재사용해 compile predicate/prior-mask 검사를 한 번만 수행한다.

Current와 Improved의 retained CSR과 query 작업량은 같다. query 평균 시간의 차이를 Improved가 더 많은 불가능 간선을 삭제한 결과로 설명하면 안 된다. bitmap은 생성 중 일시적으로만 보유하며 위 공간값은 allocator/RSS peak가 아니다. 실제 검색에서 template 하나가 몇 번 재사용되는지에 따라 비용 회수 정도가 달라진다.

## 5. 일반 제품 oracle의 end-to-end 비교

검색 타이머는 실제 `execute` 전체를 감싸므로 그 안의 template 생성도 포함한다. microbenchmark의 생성시간을 아래 검색시간에 더하지 않는다. 완료 결과의 evidence hashing 시간은 검색 타이머 밖이다.

### 5.1 P7: 3모드 8회

`p7-pruning-comparison.json`의 Rust comparator는 `equivalent=true`, `differences=[]`를 반환했다. 실행 순서는 Off–Current–Current–Off, Current–Improved–Improved–Current다. 단위는 ms이며 표시 자리수만 정리했다.

| 모드 | n | 실제 search samples | Rust 평균 |
| --- | ---: | --- | ---: |
| Off | 2 | 15.616216, 13.346541 | 14.4813785 |
| Current | 4 | 16.001277, 13.379711, 12.723301, 18.581481 | 15.1714425 |
| Improved | 2 | 12.227743, 14.239929 | 13.233836 |

Rust가 출력한 변화율은 Current/Off `+4.7651817125006435%`, Improved/Current `-12.771405883125485%`, Improved/Off `-8.614804868196767%`다. 이는 해당 작은 표본의 기술 통계다. 전체 검색이 십여 ms 규모이고 반복 편차도 있으므로 큰 검색의 안정적인 개선율로 확대하지 않는다.

8회 모두 packing candidate 2,260개, 완성 identity 246개, 원래 queue 5,040개 전체 coverage를 보고했다. full materialized identity SHA256은 `5d680f841536522274f458a50f2bebaa1c12cb97d8f1eb401a142186df491bc9`, global coverage SHA256은 `6a2f8f7821d4b0ea5e3b236e05651b88d24047a20fa8fd9ca993f4f9aa9f2661`이다.

### 5.2 P7P4: 3모드 8회, 두 ABBA block을 분리한 비교

전체 8회인 `p7p4-pruning-comparison.json`, 첫 block의 `p7p4-pruning-off-current.json`, 둘째 block의 `p7p4-pruning-current-improved.json`은 모두 `equivalent=true`, `differences=[]`를 반환했다. 같은 q1 바이너리와 9-worker 계약을 유지했고, 실행 순서는 Off–Current–Current–Off, Current–Improved–Improved–Current다.

| ABBA block | 모드·회차 | n | 실제 search samples, ms | 해당 block의 Rust 평균, ms |
| --- | --- | ---: | --- | ---: |
| Off–Current | Off 1/2 | 2 | 78,097.005293, 87,084.931992 | 82,590.9686425 |
| Off–Current | Current 1/2 | 2 | 78,301.723727, 81,496.605538 | 79,899.1646325 |
| Current–Improved | Current 3/4 | 2 | 72,533.646396, 74,178.360796 | 73,356.003596 |
| Current–Improved | Improved 1/2 | 2 | 72,100.910837, 70,057.765284 | 71,079.3380605 |

첫 block의 `time_change_percent.product_current_vs_off`는 `-3.259198982944045%`다. 첫 pair에서는 Current가 Off보다 오래 걸렸고, 같은 Off의 두 실행도 크게 달랐다. 이 block만으로 가지치기 유무의 안정적인 개선율을 선언하지 않는다.

제품 기본 선택에 사용한 비교는 **둘째 block의 Current 3/4 대 Improved 1/2**다. `p7p4-pruning-current-improved.json`의 `time_change_percent.product_improved_vs_current`와 `treatment_time_change_percent`는 모두 `-3.103584470111642%`다. 이 관측된 약 3.10%의 총시간 감소와 동일 CSR을 만드는 compile 판정의 중복 제거를 함께 근거로, 현재 소스에서 Improved를 기본 모드로 채택했다. 검색 타이머에는 template 생성부터 결과 조립까지 포함된다. 각 모드 2회라는 표본 한계는 남으며 모든 workload에 같은 개선율을 약속하지 않는다.

전체 8회를 한 번에 묶으면 Current 평균은 `76,627.58411425 ms`, Improved 평균은 `71,079.3380605 ms`이고 변화율은 `-7.240533703212792%`로 출력된다. 그러나 Current 1/2회차의 환경과 Current 3/4회차의 환경 사이에 시간 드리프트가 관측된다. 이 합산은 앞 block의 느린 Current 표본도 포함하므로 **약 7.24%를 Improved의 주효과로 보고하지 않는다**. 전체 comparator는 8회 결과의 공통 계약을 확인하는 데 사용하고, 기본 선택의 시간 해석은 같은 block으로 제한한다.

첫 Off–Current comparator의 일반 `primary_comparison`은 두 product arm만 있을 때 실험용 route=0 기본 label을 남기며 `treatment_time_change_percent`는 null이다. 첫 block 표는 실제 product group의 sample/mean과 명시적인 `product_current_vs_off` 항목을 사용했다. 둘째 block comparator는 실제 `product_prefix_route=1`의 Current/Improved를 주 비교로 기록한다.

### 5.3 정확히 확인한 결과와 digest 경계

P7P4 8개 실행의 공통 증거는 다음과 같다.

| 항목 | 기록값 |
| --- | --- |
| packing candidate count | 29,856,840 |
| complete materialized identity count | 456,459, `covers_reported_family=true` |
| identity SHA256, ordered CTK1 | `08c477a2f71455a72778f076f9942789a9997ab3b3008fc2a3d043f02458f062` |
| normalized solution-set hash | `cts1:2fcbc950acdffa31` |
| 원래 queue / covered queue count | 4,233,600 / 4,233,600 |
| global coverage SHA256 | `0d7794467640c727b89ccd5f170fc4430228e0aba6fa9d7f8657b0e653ef3912` |
| geometry catalog digest | `738a8ea93f0f82fb` |
| 실제 worker / active worker | 9 / 9 |
| BuildUp node / concrete reachability state | 0 / 0 |
| complete / resource truncated | count·packing·probability complete, `resource_truncated=false` |

**전체 candidate-set digest를 계산한 실행은 아니다.** P7과 P7P4의 제품 보고서에는 `packing_candidate_set_digest="not-calculated"`, `packing_candidate_set_digest_calculated="false"`가 있다. comparator가 같은 문자열을 확인해도 실제 candidate-set digest 검증이 되지는 않는다. catalog digest 역시 catalog를 식별하며 생산된 모든 candidate tuple의 집합 digest를 대신하지 않는다.

따라서 이 측정의 정확성 근거는 같은 입력·소스·바이너리·CPU, 보존된 candidate count, 완전한 결과 identity/coverage 해시, 그리고 E2 제거 및 Current/Improved의 정확한 CSR 보존 논증으로 한정한다. candidate count만으로 candidate 집합의 equality를 증명했다고 쓰지 않는다.

또한 이번 global-only comparator는 해법별 전체 queue map을 직접 비교하지 않는다. 그 실측 주장이 필요하면 별도 `--coverage-slot` capture와 Rust `--compare-coverage`가 필요하다. 원래 ordinal의 16개 sample은 전체 queue export가 아니다.

## 6. 코어 검증과 동시 테스트 자원 경합

| 검증 | 결과 | 근거 artifact |
| --- | --- | --- |
| 초기 focused tests | 31 passed, 0 failed | `focused-tests.log` |
| q2 release library, 테스트 함수 병렬 실행 | 997 passed, 75 failed, 14 ignored | `quotient2-release-tests.log` |
| 같은 q2 test binary, `--test-threads=1` | 1,072 passed, 0 failed, 14 ignored | `quotient2-isolated-tests.log` |
| 명시적 ignored microbenchmark 실행 | 1 passed, 0 failed | `kick-pruning-micro.stdout` |

첫 q2 전체 batch 실패에는 `ResourceAdmission`의 `Deferred / SharedResourceContention`가 기록되어 있다. 같은 test binary를 테스트 함수 단위로 직렬화하자 75개 실패가 해소되었다. 소스 수정이나 검색 worker 축소로 해결한 결과가 아니다. 바꾼 것은 Rust test harness의 동시 테스트 수이며, 실제 benchmark의 Auto/9-worker 검색 계약은 그대로 유지했다.

14개 ignored 항목을 전체 검증에서 실행했다고 표시하지 않는다. 그중 이 문서의 microbenchmark는 별도 명시 실행한 결과만 기록한다. 이 검증 범위는 `clearra-core-executor` release library이며 저장소 전체 crate/플랫폼 qualification 완료를 뜻하지 않는다.

q7에서는 `quotient7-observation-tests-final.log`에 관측 관련 **81 passed / 0 failed**, `quotient7-full-core-tests-fixed.log`에 전체 core lib **1,104 passed / 0 failed / 14 ignored**가 기록되어 있다. 전체 suite의 테스트 시간은 로그의 167.61초다. `quotient7-release-build.log`는 release build 완료를 기록한다. 이 회귀 통과와 §3.10의 P7P4 freeze 실패를 함께 유지하며, unit suite 통과를 큰 see-7 완주의 증거로 바꾸지 않는다.

q9의 첫 관측 batch는 `quotient9-observation-tests.log`에서 **85 passed / 1 failed**였다. `full_information_collection_uses_solved_terminal_target_not_intermediate_supply` fixture의 생성이 `NextCycleRemainingPieceCountInvalid`로 거절되었다. 수정 후 기본 기능의 `quotient9-observation-tests-fixed.log`는 **84 passed / 0 failed**, parallel/local A/B의 `quotient9-observation-parallel-tests.log`는 **86 passed / 0 failed**다. 서로 겹치는 두 feature 구성을 합쳐 170개의 독립 테스트라고 세지 않는다. `quotient9-release-build.log`는 release build 완료를 기록하며, 이 결과를 q7의 전체 core suite 1,104개와 구분한다. q9 전체 suite를 새로 완료했다는 주장은 하지 않는다.

q10의 첫 `quotient10-observation-tests.log`는 **91 passed / 1 failed**였다. 관측 probe에 `prepare_trie`가 빠져 `wasm_observation_future_class_missing`로 거절되었다. 세 probe의 준비를 연결한 뒤 `quotient10-observation-tests-fixed.log`는 **92 passed / 0 failed / 4.29초**이며 receipt는 `1795329788399199036-6`다. `quotient10-release-build.log`는 release build가 **1분 32초**에 완료됐음을 기록한다. 이 focused 결과나 build 성공을 §3.14의 미완료 P7P4 대신 전체 탐색 통과로 표시하지 않는다.

q11의 `quotient11-observation-tests.log`는 source `55ec4b7`에서 **101 passed / 0 failed / 4.58초**를 기록하며 receipt는 `1795330571780514329-5`다. `quotient11-release-build.log`는 release build **1분 44초** 완료를 기록한다. 이 focused 검증을 저장소 전체 suite 또는 전체 10/11-lock 완료 검증으로 넓혀 해석하지 않는다. q11 P7P4 mask 1/0은 §3.16과 같이 모두 미완료이며, 실제 8-lock ABBA 완료 비교는 §3.17에 별도로 기록했다.

q12/q13 통합의 `quotient13-observation-tests.log`는 **110 passed / 0 failed / 5.11초**, receipt `1795331432677972895-5`다. `quotient12-import-tests.log`는 **5 passed / 0 failed / 0.01초**, receipt `1795331502415900355-6`다. 두 focused 범위를 각각 기록하며 기존 suite 결과와 합쳐 독립 테스트 수로 부풀리지 않는다. 전체 SHA metadata를 사용한 `quotient13b-release-build.log`의 release 시간은 **24.08초**다. 첫 q13 실행의 출처 gate 실패 및 q13b 정식 측정과 구분한다.

q14는 테스트 import 누락의 최초 9개 컴파일 오류를 수정한 뒤 관측 **120 passed / 5.16초**, product suffix **3 passed / 0.00초**를 기록했다. artifact와 receipt, fresh-bag 7개 회귀의 구성 word 및 독립 경계 검증 범위는 §3.21에 명시한다. release는 **1분 44초**에 완료했다. 이전 버전의 full suite 수와 이 focused 결과를 합산하지 않으며 q14의 전체 10/11-lock 완료로 해석하지 않는다.

## 7. 보관 바이너리로 재현하는 명령

다음은 저장소 root에서 수행하는 재현 명령이다. `clearra-manage`는 사전 빌드된 Rust manager를 사용한다. 각 명령은 해당 q 버전으로 artifact에 보관한 실행 파일을 가리키며, 다른 소스로 재빌드한 파일과 같은 비교 묶음에 넣지 않는다. 이 문서 작성 과정에서 이 명령을 다시 실행하지 않았다.

### 7.1 P7/P7P4 oracle 한 arm

```sh
env -u CLEARRA_OPERATION_MASKS \
  CLEARRA_PLACEMENT_DEPENDENCY=0 \
  CLEARRA_STRUCTURAL_ORDER=0 \
  CLEARRA_STATIC_INFLUENCE=0 \
  CLEARRA_TRANSLATION_INTERVAL=0 \
  CLEARRA_STRICT_PREFIX=1 \
  CLEARRA_PRODUCT_PREFIX_ROUTE=1 \
  CLEARRA_STRICT_CATALOG_GUARDS=0 \
  CLEARRA_STRICT_LANGUAGE_FRONTIER=0 \
  CLEARRA_STRICT_SUBSUMPTION=0 \
  CLEARRA_STRICT_STAGE_PROFILE=0 \
  CLEARRA_STRICT_STAGE_LEGACY_UPPER=0 \
  CLEARRA_STRICT_GLOBAL_KICK_PRUNE=0 \
  clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 900 -- \
  _local/artifacts/see7-quotient-pruning-20261122/bin/quotient1-placement --p7
```

P7P4는 마지막 `--p7`을 제거한다. Current와 Improved는 `CLEARRA_STRICT_GLOBAL_KICK_PRUNE`만 각각 1과 2로 바꾼다. 나머지 legacy 실험 축은 기본 0으로 고정하고, memo/storage 관련 환경도 arm 사이에 바꾸지 않는다. 원래 시간 측정처럼 capture와 stage profiling은 사용하지 않는다. 보관된 q1 바이너리에는 compile-time source SHA가 이미 포함되어 있다.

### 7.2 Rust 비교기

```sh
_local/artifacts/see7-quotient-pruning-20261122/bin/quotient1-placement --compare \
  _local/artifacts/see7-quotient-pruning-20261122/p7-off-1.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7-current-1.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7-current-2.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7-off-2.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7-current-3.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7-improved-1.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7-improved-2.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7-current-4.json

_local/artifacts/see7-quotient-pruning-20261122/bin/quotient1-placement --compare \
  _local/artifacts/see7-quotient-pruning-20261122/p7p4-off-1.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7p4-current-1.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7p4-current-2.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7p4-off-2.json

_local/artifacts/see7-quotient-pruning-20261122/bin/quotient1-placement --compare \
  _local/artifacts/see7-quotient-pruning-20261122/p7p4-current-3.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7p4-improved-1.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7p4-improved-2.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7p4-current-4.json

_local/artifacts/see7-quotient-pruning-20261122/bin/quotient1-placement --compare \
  _local/artifacts/see7-quotient-pruning-20261122/p7p4-off-1.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7p4-current-1.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7p4-current-2.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7p4-off-2.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7p4-current-3.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7p4-improved-1.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7p4-improved-2.json \
  _local/artifacts/see7-quotient-pruning-20261122/p7p4-current-4.json
```

비교기는 읽기 전용이다. 서로 다른 fixture를 같은 호출에 넣지 않는다. P7P4의 앞 두 호출은 각 ABBA block의 시간 통계이며 마지막 호출은 전체 8회의 결과 계약 동등성을 확인한다. 마지막 호출의 block 혼합 변화율을 §5.2의 같은 block 비교 대신 사용하지 않는다.

### 7.3 q2 테스트·microbenchmark·see-7의 실행 형식

```sh
_local/artifacts/see7-quotient-pruning-20261122/bin/quotient2-tests --test-threads=1

clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 900 -- \
  _local/artifacts/see7-quotient-pruning-20261122/bin/quotient2-tests \
  global_kick_pruning_generation_and_query_cost --ignored --nocapture --test-threads=1

env CLEARRA_STRICT_GLOBAL_KICK_PRUNE=1 \
  clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 7200 -- \
  _local/artifacts/see7-quotient-pruning-20261122/bin/quotient2-observation p7p4 visible-7
```

관측 benchmark는 JSON의 `complete=true`와 프로세스 exit code 0을 함께 확인해야 한다. `coverage_semantics`와 BuildUp 미사용 검사는 JSON 출력 뒤에도 수행된다. 위 q2 전체 실행은 §3.1의 work-limit 실패를 재현하는 코드 상태이며, 성공 완료 예제가 아니다.

### 7.4 q3 실제 8-lock 두 경로와 관측 비교기

```sh
env CLEARRA_STRICT_GLOBAL_KICK_PRUNE=1 \
  CLEARRA_OBSERVATION_BOARD_QUOTIENT=0 \
  clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 7200 -- \
  _local/artifacts/see7-quotient-pruning-20261122/bin/quotient3-observation eight visible-7

env CLEARRA_STRICT_GLOBAL_KICK_PRUNE=1 \
  CLEARRA_OBSERVATION_BOARD_QUOTIENT=1 \
  clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 7200 -- \
  _local/artifacts/see7-quotient-pruning-20261122/bin/quotient3-observation eight visible-7

_local/artifacts/see7-quotient-pruning-20261122/bin/compare-observation \
  _local/artifacts/see7-quotient-pruning-20261122/quotient3-eight-factored.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient3-eight-board.json
```

### 7.5 q4의 같은 board 경로에서 kind union 비교

```sh
env CLEARRA_STRICT_GLOBAL_KICK_PRUNE=1 \
  CLEARRA_OBSERVATION_BOARD_QUOTIENT=1 \
  CLEARRA_OBSERVATION_KIND_UNION=0 \
  clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 7200 -- \
  _local/artifacts/see7-quotient-pruning-20261122/bin/quotient4-observation eight visible-7

env CLEARRA_STRICT_GLOBAL_KICK_PRUNE=1 \
  CLEARRA_OBSERVATION_BOARD_QUOTIENT=1 \
  CLEARRA_OBSERVATION_KIND_UNION=1 \
  clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 7200 -- \
  _local/artifacts/see7-quotient-pruning-20261122/bin/quotient4-observation eight visible-7

_local/artifacts/see7-quotient-pruning-20261122/bin/compare-observation \
  _local/artifacts/see7-quotient-pruning-20261122/quotient4-eight-alternatives.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient4-eight-union.json
```

q4 P7P4는 kind union 1의 명령에서 `eight`을 `p7p4`로 바꾼 실행에 해당한다. 결과는 §3.4의 suffix-limit 실패이며 성공 완료를 재현하는 명령으로 취급하지 않는다.

### 7.6 q5의 같은 board/kind에서 source prefix 비교

```sh
env CLEARRA_STRICT_GLOBAL_KICK_PRUNE=1 \
  CLEARRA_OBSERVATION_BOARD_QUOTIENT=1 \
  CLEARRA_OBSERVATION_KIND_UNION=1 \
  CLEARRA_OBSERVATION_SOURCE_PREFIX=0 \
  clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 7200 -- \
  _local/artifacts/see7-quotient-pruning-20261122/bin/quotient5-observation eight visible-7

_local/artifacts/see7-quotient-pruning-20261122/bin/compare-observation \
  _local/artifacts/see7-quotient-pruning-20261122/quotient5-eight-source-off.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient5-eight-source-on.json
```

source prefix 1은 첫 명령의 `CLEARRA_OBSERVATION_SOURCE_PREFIX`만 1로 바꾼다. q5 P7P4의 실패 실행은 source prefix 1에서 `eight`을 `p7p4`로 바꾼 명령이다. q5의 보관 바이너리는 관측 schema v4이며 boundary suffix 축을 지원하는 q6 바이너리와 구분한다.

### 7.7 q6의 같은 board/kind/source에서 boundary suffix 비교

```sh
env CLEARRA_STRICT_GLOBAL_KICK_PRUNE=1 \
  CLEARRA_OBSERVATION_BOARD_QUOTIENT=1 \
  CLEARRA_OBSERVATION_KIND_UNION=1 \
  CLEARRA_OBSERVATION_SOURCE_PREFIX=1 \
  CLEARRA_OBSERVATION_BOUNDARY_SUFFIX=1 \
  clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 7200 -- \
  _local/artifacts/see7-quotient-pruning-20261122/bin/quotient6-observation eight visible-7

_local/artifacts/see7-quotient-pruning-20261122/bin/compare-observation \
  _local/artifacts/see7-quotient-pruning-20261122/quotient6-eight-boundary-off.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient6-eight-boundary-on.json
```

boundary 0은 첫 명령에서 `CLEARRA_OBSERVATION_BOUNDARY_SUFFIX`만 0으로 바꾼다. q6 P7P4의 실패 실행은 boundary 1에서 `eight`을 `p7p4`로 바꾼 명령이다. 보관된 q6 바이너리는 관측 schema v5이며 edge closure 실험은 포함하지 않는다.

### 7.8 q7 P7P4 edge closure 실패 실행

```sh
env CLEARRA_STRICT_GLOBAL_KICK_PRUNE=1 \
  CLEARRA_OBSERVATION_BOARD_QUOTIENT=1 \
  CLEARRA_OBSERVATION_KIND_UNION=1 \
  CLEARRA_OBSERVATION_SOURCE_PREFIX=1 \
  CLEARRA_OBSERVATION_BOUNDARY_SUFFIX=1 \
  CLEARRA_OBSERVATION_EDGE_CLOSURE=1 \
  clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 7200 -- \
  _local/artifacts/see7-quotient-pruning-20261122/bin/quotient7-observation p7p4 visible-7
```

이 명령은 §3.10의 freeze work-limit 실패에 해당한다. q7의 보관 바이너리는 schema v6이며 이후 수집 단축이나 hybrid mode 2 실험과 같은 바이너리가 아니다.

### 7.9 q9 P7P4 hybrid edge 실패와 8-lock 비교 형식

```sh
env CLEARRA_STRICT_GLOBAL_KICK_PRUNE=1 \
  CLEARRA_OBSERVATION_BOARD_QUOTIENT=1 \
  CLEARRA_OBSERVATION_KIND_UNION=1 \
  CLEARRA_OBSERVATION_SOURCE_PREFIX=1 \
  CLEARRA_OBSERVATION_BOUNDARY_SUFFIX=1 \
  CLEARRA_OBSERVATION_EDGE_CLOSURE=2 \
  CLEARRA_OBSERVATION_COLLECT_FULL=1 \
  clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 7200 -- \
  _local/artifacts/see7-quotient-pruning-20261122/bin/quotient9-observation p7p4 visible-7
```

이 명령은 §3.11의 정책 kind union work-limit 실패에 해당한다. §3.12의 완료된 8-lock 비교는 fixture를 `eight`으로 바꾸고 `(EDGE_CLOSURE,COLLECT_FULL)`만 `(0,0),(1,0),(2,0),(2,1)`로 바꾼다. q9 보관 바이너리는 schema v7이며 q10의 공급 multiset 축은 포함하지 않는다.

```sh
_local/artifacts/see7-quotient-pruning-20261122/bin/compare-observation \
  _local/artifacts/see7-quotient-pruning-20261122/quotient9-eight-edge0-collect0.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient9-eight-edge1-collect0.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient9-eight-edge2-collect0.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient9-eight-edge2-collect1.json
```

### 7.10 q10 공급 multiset 실험의 재현 형식

```sh
env CLEARRA_STRICT_GLOBAL_KICK_PRUNE=1 \
  CLEARRA_OBSERVATION_BOARD_QUOTIENT=1 \
  CLEARRA_OBSERVATION_KIND_UNION=1 \
  CLEARRA_OBSERVATION_SOURCE_PREFIX=1 \
  CLEARRA_OBSERVATION_BOUNDARY_SUFFIX=1 \
  CLEARRA_OBSERVATION_EDGE_CLOSURE=2 \
  CLEARRA_OBSERVATION_COLLECT_FULL=1 \
  CLEARRA_OBSERVATION_SUPPLY_MULTISET=1 \
  clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 7200 -- \
  _local/artifacts/see7-quotient-pruning-20261122/bin/quotient10-observation p7p4 visible-7
```

이는 §3.14의 work-limit 실패를 재현하는 형식이다. §3.15의 완료된 같은 바이너리 8-lock 비교는 fixture를 `eight`으로 바꾸고 `SUPPLY_MULTISET`만 0, 1, 1, 0의 ABBA 순서로 바꿨다. q10 보관 바이너리는 schema v8이며 q11의 prefix mask 축은 포함하지 않는다. 완료 JSON의 비교 명령은 다음과 같다.

```sh
_local/artifacts/see7-quotient-pruning-20261122/bin/compare-observation \
  _local/artifacts/see7-quotient-pruning-20261122/quotient10-eight-multiset0a.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient10-eight-multiset1a.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient10-eight-multiset1b.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient10-eight-multiset0b.json
```

### 7.11 관측 비교기의 입력 계약

독립 Rust 소스는 같은 artifact root의 `compare_observation.rs`다. q3의 최초 비교 JSON은 출력 v1, q4의 최초 비교는 출력 v2, q4 재검증과 q5의 비교는 출력 v3, q6의 비교는 출력 v4다. 비교 출력 v11은 실제 v12 q15 입력을, 비교 출력 v12는 실제 v13 q16 입력을 검증했다. 최신 비교 출력 v13은 실제 v14 q17 입력까지 검증했다. q17의 on arm 모드 key는 `board=1,kind=1,source=1,boundary=1,edge=2,collect=1,multiset=1,prefix=0,suffix=1,prereveal=0,existence=1,filter=1,groups=0,branch_bound=1`처럼 열네 축을 드러낸다. `changed_axes`에는 실제로 바뀐 축만 나오며 나머지 축을 고정한 비교를 구분한다. 이전 schema에 없는 축은 `legacy`로 표시하고 특정 모드로 추론하지 않는다. schema·source commit·binary SHA·fixture·policy·CPU·kick mode의 동일성은 계속 필수다. 관측 producer·consumer의 비용 계측은 기대되는 측정 차이로 분리하고 완료·coverage·확률 의미 gate는 유지한다.

v6부터는 `observation_board_closure_pose_checks`, `observation_board_closure_harddrop_hits`, `observation_board_closure_reach_queries`, `observation_board_closure_visited_states`, `observation_board_closure_parent_boards`의 다섯 계측을 canonical unsigned 정수로 요구한다. v7부터는 `observation_collection_calls`, `observation_collection_full_subtrees`도 같은 형식으로 요구한다. v8부터는 `observation_supply_multiset_` 접두사의 `checked_roots`, `pruned_roots`, `certificate_nodes`, `certificate_edges`, `aliases` 다섯 계측을 추가로 요구한다. v9는 `observation_prefix_mask_` 접두사의 `lookups`, `hits`, `misses`, `full_fallbacks`, `words`, `work` 여섯 계측도 요구한다. v7 이하의 공급 multiset 축과 v8 이하의 prefix mask 축은 명시적인 `legacy`다.

v10은 `suffix_import_mode`와 `pre_reveal_memo_mode`를 추가하며 v9 이하는 두 축 모두 `legacy`다. `observation_suffix_import_`의 source/reachable/inserted node·edge 수, root/node/edge visit, mapping slot, before/after/peak bytes의 13개 필드와 `observation_pre_reveal_`의 probes/hits/inserts/full_fallbacks/retained_bytes/peak_bytes 여섯 필드를 canonical unsigned 정수로 요구한다. 두 새 모드의 제품 기본은 0이고 mode 1에는 local A/B 권위가 필요하다. suffix import 방문 계측은 별도 kind import 비용이며 기존 누적 physical work를 초기화하거나 대체하지 않는다. 공개 전 memo는 기존 DP memo 회계를 따르고, 이미 끝난 language work를 차감하지 않는다. memo retained bytes는 evaluator 보유량에 포함되므로 중복 합산하지 않는다.

v11은 `source_existence_mode=0/1`을 추가하며 v10 이하는 `legacy`다. 여섯 `observation_source_existence_` 계측 `candidates`, `kind_rejections`, `relation_rejections`, `accepted_candidates`, `prefix_checks`, `language_edges`를 canonical unsigned 정수로 요구한다. top metadata의 `source_sequence_length`는 usize 범위 정수, 두 projection은 boolean이어야 하며 full result 값 및 같은 비교 묶음의 다른 실행과 일치해야 한다. 이전 schema에는 이 새 top metadata를 요구하지 않는다. source existence의 기본은 0이고 1에는 local A/B 권위가 필요하다.

source mode 1에서 VisibleSeven·board 1·boundary 1의 eight/P7P4/eleven은 각각 **N=9/11/12**, 두 projection false, accepted > 0 및 `accepted_candidates == observation_board_candidate_count`를 요구한다. source-existence 축만 다른 완료 pair에서는 mode 1의 accepted 수와 mode 0의 board 후보 수도 비교한다. 이는 별도 **count 진단**이며 후보 집합 digest나 전 경로 증명이 아니다. `coverage_row_count`와 `pattern_verified_execution_count`는 기존 실행 진단값으로 남기므로 oracle 단계가 실제로 생략되면 0일 수 있다. 전체 coverage·확률·완료 gate를 그 진단값으로 대체하지 않는다.

v12는 `source_filter_mode=0/1`과 네 `observation_source_filter_` 계측 `calls`, `mask_words`, `domain_words`, `prefix_checks`를 추가한다. v11 이하는 filter 축을 `legacy`로 두며 기본 0, mode 1은 local A/B 권위를 요구한다. VisibleSeven의 eight/P7P4/eleven에서 filter 1은 source existence 1·board 1·boundary 1, 원래 N과 두 projection false, H=N−7≤5, accepted > 0 및 calls == accepted를 요구한다. filter 축만 다른 완료 pair도 기존 source-existence 축과 같은 범위의 후보 수 진단을 별도로 적용한다. 이 계측은 source compiler의 구조 작업량이며 기존 physical 64M에 더하거나 상한을 재정의하지 않는다. q15 certificate가 중복 cap 검사를 실제로 생략하면 기존 q5/q11 관련 작업 계측이 0일 수 있다.

v13은 `multiset_groups_mode=0/1`과 아홉 `observation_multiset_group_` 정수 계측 `builds`, `build_root_visits`, `key_lookups`, `matched_root_visits`, `unknown_root_visits`, `fallbacks`, `retained_members`, `work`, `evictions`를 요구한다. v12 이하는 groups 축이 `legacy`이며 기본 0, mode 1은 local A/B와 supply multiset 1·board 1·kind union 1을 요구한다. 완료한 VisibleSeven eight/P7P4의 mode 1에서는 builds > 0을 요구하되 실패 실행이나 그 밖의 fixture에는 양수 조건을 적용하지 않는다. groups 축만 다른 동일 계약 완료 pair는 accepted 및 board 후보 수를 각각 비교하며 이를 전체 candidate-set 검증으로 표현하지 않는다. group work는 기존 kind-union work에 포함된 부분량이므로 다시 더하지 않는다. 선택적인 group member는 기존 8M member 상한을 공유하고 필요 시 evict하며 원래 exact 언어를 버리지 않는 계약을 유지한다. q16 실제 v13 ABBA에서 이 gate들을 통과했다.

v14는 `branch_bound_mode=0/1`과 네 정수 계측 `observation_branch_bound_attempts`, `observation_branch_bound_pruned_actions`, `observation_branch_bound_skipped_observations`, `observation_branch_bound_checks`를 추가한다. v13 이하는 branch-bound 축을 `legacy`로 두고 기본 0, mode 1은 local A/B를 요구한다. 완료한 VisibleSeven eight/P7P4의 mode 1에서는 attempts > 0을 요구하며 실패나 다른 fixture에는 양수 조건을 강제하지 않는다. 이 정책 진단값은 기대되는 측정 차이로 분리하고 전체 원본 coverage·확률·완료·기존 자원 gate를 유지한다. 실제 q17 8-lock ABBA에서 이 gate들을 통과했다.

edge mode 0, collect mode 1, supply multiset mode 0, prefix mask mode 0은 제품 기본이다. edge 1/2, collect 0, supply multiset 1 또는 prefix mask 1에는 A/B 기능 권위가 필요하며, edge 2는 v7 이상에서 허용한다. 앞선 축의 기존 허용 조건은 유지한다. prefix mask의 work는 board work의 부분량이며 비교기의 evidence scope도 중복 합산을 금지한다.

관측 비교기는 유효한 완료 실행의 시간 목록과 평균을 출력한다. 전체 입력이 완료·동등성·같은 실행 계약을 통과하고 두 모드가 정확히 한 축만 다를 때 `single_axis_completed_mean_ratios`에 treatment 평균/reference 평균의 기술 통계를 추가한다. 허용 방향은 0→1이며 edge 축은 0→2와 1→2도 허용한다. 다른 축은 고정되어야 하며 reference 평균이 0이면 비율을 만들지 않는다. 실패 종료시간은 평균·비율에서 제외한다. 이 비율은 반복성·유의성 추정이나 여러 축이 바뀐 효과 분해가 아니다.

한 조합만 입력하면 `completion-only`, `equivalent=null`이므로 10/11-lock 단독 완주 기록을 기존 경로와의 동등성 검증으로 잘못 표시하지 않는다. q9의 v7 입력 네 arm은 기존 비교 출력 v6의 `quotient9-eight-comparison.json`에서 실제로 검증했다. 입력 v8을 지원하는 소스는 `recursion_limit=256`을 명시한 뒤 컴파일을 완료했고, q10의 v8 입력 ABBA 네 arm은 비교 출력 v7의 `quotient10-eight-comparison.json`에서 실제 `validation_passed=true / equivalent=true`를 확인했다. v9 입력을 지원하는 비교 출력 v8도 컴파일을 완료했으며 `quotient11-eight-comparison.json`에서 q11 ABBA 네 arm의 완료·동등성 gate를 통과했다. 기존 비교 JSON은 보존하고 새 버전의 출력은 별도 파일에 기록한다.

v10 입력을 지원하는 비교 출력 v9는 첫 q13 예비 자료에서 짧은 source SHA를 정확히 거절했다. 원본 result differences가 비어 있어도 source 출처 gate를 완화하지 않는다. 전체 source SHA를 기록한 `quotient13b-eight-comparison.json`에서는 정식 다섯 arm의 완료·동등성 gate를 통과했다. 같은 결과에서 계산한 memo ABBA 비율과 import의 비대칭 표본 비율은 실험 배치를 구분해 해석한다.

v11 입력을 지원하는 비교 출력 v10은 컴파일을 완료했고 `quotient13b-eight-comparison-v10.json`에서 기존 v10 다섯 arm의 backward comparison을 실제로 통과했다. `quotient14-eight-comparison.json`에서는 실제 v11 source-existence ABBA 네 arm의 완료·동등성 및 source accepted 대 reference board count 진단도 통과했다.

v12 입력을 지원하는 비교 출력 v11도 컴파일을 완료했고 `quotient14-eight-comparison-v11.json`에서 같은 q14 네 arm의 backward comparison을 통과했다. 실제 v12 입력의 `quotient15-eight-comparison.json`은 source-filter 0/1 ABBA의 완료·동등성 gate, qualified calls==accepted, 네 교차 pair의 candidate count 진단을 통과했다.

v13 입력을 지원하는 비교 출력 v12도 컴파일을 완료했다. `quotient15-eight-comparison-v12.json`은 기존 q15 v12 네 arm의 backward comparison을 통과했다. 실제 v13 입력의 `quotient16-eight-comparison.json`도 `validation_passed=true / equivalent=true`와 빈 contract/result differences를 기록하며 group 생성 여부와 각 accepted·board 후보 수 진단을 통과했다.

v14 입력을 지원하는 비교 출력 v13은 `quotient16-eight-comparison-v13.json`에서 기존 q16 네 arm의 backward comparison을 통과했다. 실제 v14 입력의 `quotient17-eight-comparison.json`도 완료·동등성·branch-bound attempts gate를 통과했다. 다른 source·binary의 결과를 같은 비교에 섞지 않는다.

후속 비교기 소스는 입력 **v15 / 출력 v14**의 열다섯 번째 `prefix_cones_mode=0/1` 축과 다섯 정수 계측 `observation_prefix_cone_{lookups,hits,misses,replacements,publishes}`를 추가했다. v14 이하는 cone 축을 `legacy`로 표시한다. Mode 1의 VisibleSeven eight/P7P4/eleven은 source filter·existence·board·boundary가 모두 1, 두 projection false, `H=N−7`이 1~5임을 요구한다. 완료한 eight/P7P4에서는 lookups와 publishes가 양수여야 하지만 hits가 0이어도 유효하다. 이 계측은 기대되는 차이로 분리하며 원래 coverage·확률·완료·resource gate를 유지한다. 모드 비율은 나머지 열네 축이 고정된 단일 축 비교에서만 낸다.

v15에서는 후속 제품의 branch-bound 기본을 1로 채택하는 계약을 반영하며 local A/B의 0 override를 구분한다. 과거 v14는 기본 0·mode 1 local A/B 계약을 그대로 유지하고 어느 schema에서도 실제 모드값을 추정하지 않는다. **후속 비교기 컴파일과 기존 q17 네 arm의 backward comparison을 완료했다.** `quotient17-eight-comparison-v14.json`은 출력 v14·`validation_passed=true / equivalent=true`·빈 contract/result differences를 기록한다. 실제 v15 입력 검증은 대기이며 이 backward comparison을 q18 제품 build 또는 실제 성능 검증 완료로 표시하지 않는다.

### 7.12 q11 prefix mask 실험의 재현 형식

```sh
env CLEARRA_STRICT_GLOBAL_KICK_PRUNE=1 \
  CLEARRA_OBSERVATION_BOARD_QUOTIENT=1 \
  CLEARRA_OBSERVATION_KIND_UNION=1 \
  CLEARRA_OBSERVATION_SOURCE_PREFIX=1 \
  CLEARRA_OBSERVATION_BOUNDARY_SUFFIX=1 \
  CLEARRA_OBSERVATION_EDGE_CLOSURE=2 \
  CLEARRA_OBSERVATION_COLLECT_FULL=1 \
  CLEARRA_OBSERVATION_SUPPLY_MULTISET=1 \
  CLEARRA_OBSERVATION_PREFIX_MASK=1 \
  clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 7200 -- \
  _local/artifacts/see7-quotient-pruning-20261122/bin/quotient11-observation p7p4 visible-7
```

같은 보관 바이너리에서 `PREFIX_MASK=0`으로 바꾼 대조도 §3.16과 같이 work-limit으로 미완료다. §3.17의 완료된 8-lock ABBA는 fixture를 `eight`으로 바꾸고 prefix mask만 0, 1, 1, 0 순서로 실행했다. 비교 명령은 다음과 같다.

```sh
_local/artifacts/see7-quotient-pruning-20261122/bin/compare-observation \
  _local/artifacts/see7-quotient-pruning-20261122/quotient11-eight-mask0a.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient11-eight-mask1a.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient11-eight-mask1b.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient11-eight-mask0b.json
```

### 7.13 q13b subset import·공개 전 memo의 재현 형식

```sh
env CLEARRA_STRICT_GLOBAL_KICK_PRUNE=1 \
  CLEARRA_OBSERVATION_BOARD_QUOTIENT=1 \
  CLEARRA_OBSERVATION_KIND_UNION=1 \
  CLEARRA_OBSERVATION_SOURCE_PREFIX=1 \
  CLEARRA_OBSERVATION_BOUNDARY_SUFFIX=1 \
  CLEARRA_OBSERVATION_EDGE_CLOSURE=2 \
  CLEARRA_OBSERVATION_COLLECT_FULL=1 \
  CLEARRA_OBSERVATION_SUPPLY_MULTISET=1 \
  CLEARRA_OBSERVATION_PREFIX_MASK=1 \
  CLEARRA_OBSERVATION_SUFFIX_IMPORT=1 \
  CLEARRA_OBSERVATION_PRE_REVEAL_MEMO=1 \
  clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 600 -- \
  _local/artifacts/see7-quotient-pruning-20261122/bin/quotient13b-observation p7p4 visible-7
```

이는 §3.20의 work-limit 실패를 재현하는 형식이다. §3.19의 완료된 `eight` 비교는 같은 보관 바이너리에서 memo 0, 1, 1, 0으로 ABBA를 실행하고 마지막에 memo 1·suffix import 0 대조를 추가했다. 완료 JSON의 비교 명령은 다음과 같다.

```sh
_local/artifacts/see7-quotient-pruning-20261122/bin/compare-observation \
  _local/artifacts/see7-quotient-pruning-20261122/quotient13b-eight-memo0a.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient13b-eight-memo1a.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient13b-eight-memo1b.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient13b-eight-memo0b.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient13b-eight-import0.json
```

### 7.14 q15 source-qualified filter의 재현 형식

아래 환경 변수와 보관 바이너리·인자는 P7P4 receipt의 실제 child command에 해당한다. 감독 timeout은 600초다.

```sh
clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 600 -- \
  env CLEARRA_STRICT_GLOBAL_KICK_PRUNE=1 \
  CLEARRA_OBSERVATION_BOARD_QUOTIENT=1 \
  CLEARRA_OBSERVATION_KIND_UNION=1 \
  CLEARRA_OBSERVATION_SOURCE_PREFIX=1 \
  CLEARRA_OBSERVATION_BOUNDARY_SUFFIX=1 \
  CLEARRA_OBSERVATION_EDGE_CLOSURE=2 \
  CLEARRA_OBSERVATION_COLLECT_FULL=1 \
  CLEARRA_OBSERVATION_SUPPLY_MULTISET=1 \
  CLEARRA_OBSERVATION_PREFIX_MASK=1 \
  CLEARRA_OBSERVATION_SUFFIX_IMPORT=1 \
  CLEARRA_OBSERVATION_PRE_REVEAL_MEMO=0 \
  CLEARRA_OBSERVATION_SOURCE_EXISTENCE=1 \
  CLEARRA_OBSERVATION_SOURCE_FILTER=1 \
  _local/artifacts/see7-quotient-pruning-20261122/bin/quotient15-observation p7p4 visible-7
```

§3.26의 완료 비교는 fixture를 `eight`으로 바꾸고 `SOURCE_FILTER`만 0, 1, 1, 0 순서로 실행했다. `quotient15-eight-comparison.json`은 다음 입력 순서로 비교했다.

```sh
_local/artifacts/see7-quotient-pruning-20261122/bin/compare-observation \
  _local/artifacts/see7-quotient-pruning-20261122/quotient15-eight-filter0a.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient15-eight-filter1a.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient15-eight-filter1b.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient15-eight-filter0b.json
```

§3.29의 추가 P7P4 대조는 위 q15 실행 명령에서 `CLEARRA_OBSERVATION_SUPPLY_MULTISET=0`만 바꿨다. 해당 실행은 SIGKILL과 빈 JSON으로 종료되어 완료 비교기의 시간·동등성 입력에 포함하지 않는다.

### 7.15 q16 group index의 재현 형식

q16은 §7.14의 환경 변수들을 그대로 두고 보관 바이너리를 `bin/quotient16-observation`으로 바꾼 뒤 `CLEARRA_OBSERVATION_MULTISET_GROUPS=0/1`을 추가했다. 같은 `eight visible-7`에서 0, 1, 1, 0 순서로 ABBA를 실행했다. `p7p4 visible-7`은 group 1로 한 번 실행했으며 §3.32의 work-limit 실패다. 완료된 네 arm의 비교 명령은 다음과 같다.

```sh
_local/artifacts/see7-quotient-pruning-20261122/bin/compare-observation \
  _local/artifacts/see7-quotient-pruning-20261122/quotient16-eight-groups0a.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient16-eight-groups1a.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient16-eight-groups1b.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient16-eight-groups0b.json
```

### 7.16 q17 branch-bound의 재현 형식

8-lock ABBA는 아래 q17 명령의 `CLEARRA_OBSERVATION_BRANCH_BOUND`만 **0, 1, 1, 0** 순서로 바꾸어 실행했다. 네 원본 JSON 모두 **kick 2·prefix mask 0**이다. 이전 q15·q16의 환경을 암묵적으로 이어받지 않고 q17의 모든 모드를 명시한다. 아래는 branch-bound 1 arm의 재현 형식이다.

```sh
clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 600 -- \
  env CLEARRA_STRICT_GLOBAL_KICK_PRUNE=2 \
  CLEARRA_OBSERVATION_BOARD_QUOTIENT=1 \
  CLEARRA_OBSERVATION_KIND_UNION=1 \
  CLEARRA_OBSERVATION_SOURCE_PREFIX=1 \
  CLEARRA_OBSERVATION_BOUNDARY_SUFFIX=1 \
  CLEARRA_OBSERVATION_EDGE_CLOSURE=2 \
  CLEARRA_OBSERVATION_COLLECT_FULL=1 \
  CLEARRA_OBSERVATION_SUPPLY_MULTISET=1 \
  CLEARRA_OBSERVATION_PREFIX_MASK=0 \
  CLEARRA_OBSERVATION_SUFFIX_IMPORT=1 \
  CLEARRA_OBSERVATION_PRE_REVEAL_MEMO=0 \
  CLEARRA_OBSERVATION_SOURCE_EXISTENCE=1 \
  CLEARRA_OBSERVATION_SOURCE_FILTER=1 \
  CLEARRA_OBSERVATION_MULTISET_GROUPS=0 \
  CLEARRA_OBSERVATION_BRANCH_BOUND=1 \
  _local/artifacts/see7-quotient-pruning-20261122/bin/quotient17-observation eight visible-7
```

완료된 네 arm의 비교 명령은 다음과 같다.

```sh
_local/artifacts/see7-quotient-pruning-20261122/bin/compare-observation \
  _local/artifacts/see7-quotient-pruning-20261122/quotient17-eight-bb0a.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient17-eight-bb1a.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient17-eight-bb1b.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient17-eight-bb0b.json
```

P7P4 두 실행은 같은 q17 바이너리의 `p7p4 visible-7`에서 `CLEARRA_STRICT_GLOBAL_KICK_PRUNE=2`, `CLEARRA_OBSERVATION_PREFIX_MASK=0`, `CLEARRA_OBSERVATION_BRANCH_BOUND=1`을 고정하고 `CLEARRA_OBSERVATION_MULTISET_GROUPS`를 0, 1 순서로 실행했다. 나머지 공급·board·kind·boundary·edge·collection·suffix·source 존재성/filter 설정은 위 8-lock 명령과 같다. 두 결과는 §3.35의 work-limit 실패이므로 완료 평균·비율 비교에 넣지 않는다.

### 7.17 q18/q21/q22의 추가 계약

q18의 입력 v15는 `prefix_cones_mode=0/1`과 H 계측 다섯 개를 추가했다.
q21 입력 v16은 cone 2, `boundary_classes_mode=0/1`, `word_bitmap_mode=0/1`,
exact cone 9개·boundary 7개·word bitmap 9개의 정수 계측을 추가했다.
q22 입력 v17은 cone 3과 `observation_word_bitmap_board_`의
queries/cache_hits/builds/words/fallbacks/work 여섯 정수 계측을 추가한다.
모드 0은 각 실험의 기존 경로이며 cone 1/2/3은 각각 H direct, H+T exact,
H direct+T exact다. 세 선택 축 모두 기본 0이고 local A/B 권위가 필요하다.

v17의 query cache hits/builds는 **target-M canonical query 단위**다. 하나의
membership이 여러 target을 검사할 수 있으므로 membership queries보다 클 수 있다.
Raw QueryKey profile cache는 가능한 target과 정규화 준비를 재사용하며 65,536개로
제한한다. Full certificate가 양성을 증명하면 query bitmap을 만들지 않을 수 있다.
따라서 v17에는 query builds 양수 조건을 강제하지 않는다. 완료된 bitmap 1 실행에는
membership queries/work 양수, 현재 실제 positive activity를 요구하는 `eight visible-7`
에는 board queries/builds 양수 조건을 둔다. 역사적 v16에는 그 구현에 맞는 기존
query-build 활동 gate를 유지한다. Board work는 기존 kind work의 부분량이다.

비교 출력 v16은 입력 v2부터 v17까지 읽고 17개 축을 구분한다. 같은 block의
schema/source SHA/binary SHA/CPU/공급·projection·확률·완료·원래 coverage 검사는
유지한다. Prefix cones에만 0/1/2→3의 단일 축 평균 비율을 추가 허용했다.
다른 축은 고정되어야 하며 실패 종료시간은 계속 제외한다. Rustc 컴파일 뒤 기존
q21 cone 네 arm의 backward comparison인
`quotient21-eight-cone-comparison-v16.json`은 validation/equivalent 모두 true다.
실제 v17의 두 ABBA 및 P7P4 실행은 §3.45–3.46에 별도로 기록했다.
Backward 검증을 새 schema의 성능 근거로 대신하지 않는다.

q18/q21의 실제 실행은 §7.16의 전체 설정을 고정하고 다음 축을 명시적으로 추가했다.
q18에는 후속 schema에만 있는 class/bitmap 축을 결과에 임의로 추정해 넣지 않는다.

| Binary | 추가 축 | Fixture와 비교 |
| --- | --- | --- |
| `bin/quotient18-observation` | `CLEARRA_OBSERVATION_PREFIX_CONES=0/1` | eight cone ABBA; P7P4/eleven은 cone 1 |
| `bin/quotient21-observation` | prefix cones 1/2, boundary classes 0/1, word bitmap 0/1 | §3.41의 별도 ABBA, §3.42 첫 bitmap 실패, §3.43의 P7P4 두 실패 |
| `bin/quotient22-observation` | cone 2/3 또는 bitmap 0/1, 8-lock은 classes 0 | §3.45의 별도 ABBA; P7P4의 classes/bitmap/groups 대조는 §3.46 |

각 축의 환경 변수는 `CLEARRA_OBSERVATION_PREFIX_CONES`,
`CLEARRA_OBSERVATION_BOUNDARY_CLASSES`, `CLEARRA_OBSERVATION_WORD_BITMAP`이다.
공통 `CLEARRA_STRICT_GLOBAL_KICK_PRUNE=2`, `CLEARRA_OBSERVATION_PREFIX_MASK=0`,
`CLEARRA_OBSERVATION_BRANCH_BOUND=1`은 q18 이후에도 명시했다. 실행 wrapper는
동일한 `benchmark-search`·600초 감독, `visible-7` 인자, 원래 materialized 공급과
자동 9 workers를 사용한다. q22의 보관 SHA는 §2와 §3.44에 기록했다.

### 7.18 q23 공유 생산자의 재현 형식과 비교 계약

입력 **v18**은 `shared_producer_mode=0|1`과
`observation_shared_producer_` 접두어의 registrations, lock_entries,
pending_groups, pending_root_refs, finalized_classes, finalization_work,
retained_bytes, peak_bytes 여덟 unsigned 정수 계측을 추가한다. 비교 출력은
**v17**, 비교 축은 **18개**다. 이전 입력 v2–v17의 shared 축은 `legacy`이며
다른 schema를 같은 비교 그룹으로 합치지 않는다. 실제 q22 backward comparison과
q23 v18 완료 비교를 모두 수행했다.

Shared 1은 `local-search-ab`의 실제 native parallel 요청에서만 선택된다.
Fresh source에 대한 typed plan, source existence/filter 1, board 1,
boundary suffix 1, edge closure 2, suffix import 1, boundary classes 1 및
`1 ≤ H ≤ 5`, `H < 전체 lock 수`가 필요하다. Mode의 제품 기본은 **0**이다.
모든 canonical worker 분기의 완료를 확인한 뒤에만 pending H root 집합을
확정하며 미완료·취소·오류 owner의 부분 결과를 확률로 내보내지 않는다.
원래 physical cells와 place/clear 검증은 class remap 전에 유지한다.

공유 graph/kind/staging는 request 소유량으로 한 번만 계상하고 worker별 source,
scratch, 증분 importer map은 private 소유량에 남는다. Worker 종료 때 찍은
공유 snapshot은 서로 겹치므로 합산하지 않는다. Importer의 기존 구조 계측은
kind compilation/import domain이며 physical 64M에 이중 가산하지 않는다.
새 등록·lock·class 확정의 실제 작업은 기존 physical work 상한에 포함되고,
8M root member 한도와 node/work 상한을 그대로 유지한다. 이미 수행한 work를
환급하지 않으며, class 재사용으로 실제 폐기한 root 참조만 보유량에서 제외한다.

완료된 shared 1에서는 registrations==accepted candidates==board candidates,
lock entries≥registrations, classes≤pending groups, retained≤peak,
finalization work≤board work를 검사한다. 활동이 있는 실제 fixture의 groups,
roots, classes, finalization work, peak는 양수여야 한다. 원래 coverage SHA·
cardinality·확률·완료·BuildUp 0·source/binary SHA·CPU 계약도 그대로 검사한다.
단일 shared 0→1 pair의 후보 개수 검사는 count 진단이며 미계산 candidate-set
digest를 대신하지 않는다. 시간 비율은 같은 계약의 완료된 입력에만 계산한다.

실제 8-lock ABBA는 아래 설정에서 `CLEARRA_OBSERVATION_SHARED_PRODUCER`만
**0, 1, 1, 0**으로 바꿨다. 과거 q17의 groups 0이나 q22의 classes 0을 그대로
이어받지 않도록 q23의 모든 축을 명시한다.

```sh
clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 600 -- \
  env CLEARRA_STRICT_GLOBAL_KICK_PRUNE=2 \
  CLEARRA_OBSERVATION_BOARD_QUOTIENT=1 \
  CLEARRA_OBSERVATION_KIND_UNION=1 \
  CLEARRA_OBSERVATION_SOURCE_PREFIX=1 \
  CLEARRA_OBSERVATION_BOUNDARY_SUFFIX=1 \
  CLEARRA_OBSERVATION_EDGE_CLOSURE=2 \
  CLEARRA_OBSERVATION_COLLECT_FULL=1 \
  CLEARRA_OBSERVATION_SUPPLY_MULTISET=1 \
  CLEARRA_OBSERVATION_PREFIX_MASK=0 \
  CLEARRA_OBSERVATION_SUFFIX_IMPORT=1 \
  CLEARRA_OBSERVATION_PRE_REVEAL_MEMO=0 \
  CLEARRA_OBSERVATION_SOURCE_EXISTENCE=1 \
  CLEARRA_OBSERVATION_SOURCE_FILTER=1 \
  CLEARRA_OBSERVATION_MULTISET_GROUPS=1 \
  CLEARRA_OBSERVATION_BRANCH_BOUND=1 \
  CLEARRA_OBSERVATION_PREFIX_CONES=3 \
  CLEARRA_OBSERVATION_BOUNDARY_CLASSES=1 \
  CLEARRA_OBSERVATION_WORD_BITMAP=0 \
  CLEARRA_OBSERVATION_SHARED_PRODUCER=1 \
  _local/artifacts/see7-quotient-pruning-20261122/bin/quotient23-observation eight visible-7
```

완료된 네 arm의 실제 비교 명령은 다음과 같다.

```sh
_local/artifacts/see7-quotient-pruning-20261122/bin/compare-observation \
  _local/artifacts/see7-quotient-pruning-20261122/quotient23-eight-shared0a.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient23-eight-shared1a.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient23-eight-shared1b.json \
  _local/artifacts/see7-quotient-pruning-20261122/quotient23-eight-shared0b.json
```

P7P4 두 실행은 같은 설정의 `p7p4 visible-7`에서 shared 1을 고정하고 groups를
0, 1로 바꿨다. §3.49의 미완료 자료이므로 위 완료 평균에 넣지 않는다.
11-lock은 같은 설정의 `eleven visible-7`, groups 1로 실행 중이다.

## 8. 후속 결과와 남은 계약

- [x] 수정 파일: 이 문서, 구현 내용: `quotient2-p7p4-visible7.json` 및 stderr의 최종 실패 상태를 반영했다. registration/merge 통과와 초기 평가 work-limit을 구분하며 완료 확률이나 개선율을 기재하지 않았다.
- [x] 수정 파일: 이 문서와 `placement_graph_bench.rs`의 실행 artifact, 구현 내용: P7P4 Improved 반복 결과를 추가하고 8회 전체 및 두 ABBA block의 결과·CPU 계약을 Rust comparator로 확인했다. 시간 해석은 같은 block의 Current/Improved 약 3.10% 감소를 중심으로 제한했다.
- [x] 수정 파일: `reachability.rs` 및 이 문서, 구현 내용: microbenchmark의 생성비용과 P7P4 전체 검색시간을 구분하여 Improved를 제품 기본으로 채택했다. 기존 보관 바이너리의 모드와 후속 소스 기본값을 구분한다.
- [x] 수정 파일: `queue_observation_policy.rs`, `queue_observation_board_actions.rs` 및 관계 producer, 구현 내용: complete source-relative family 권한에 기반한 q3의 실제 8-lock 두 경로 완료와 전체 원본 coverage SHA 일치를 확인했다. board 경로가 더 느렸고 정책 상태가 증가했다는 결과를 §3.2에 기록했다.
- [x] 수정 파일: 관측 kind relation producer·소비자와 이 문서, 구현 내용: q4 revealed kind union의 같은 바이너리 8-lock 비교를 완료하고 원래 coverage SHA·확률과 BuildUp 미사용 계약을 확인했다. 정책 상태·검사·총시간 감소는 이 완료 fixture에서 관측한 결과다.
- [x] 수정 파일: 이 문서, 구현 내용: q4 P7P4의 board suffix-limit 실패를 부분 producer 계측과 함께 기록했다. 완료 확률이나 실패시간 speedup으로 해석하지 않는다.
- [x] 수정 파일: 이 문서와 독립 비교기, 구현 내용: q5 P7P4의 incoming shard work 사전 청구 실패를 기록하고, 새 비교기 출력 v3로 기존 q4 완료 pair의 동등성을 재확인했다.
- [x] 수정 파일: source prefix producer·소비자와 이 문서, 구현 내용: q5의 실제 8-lock source prefix 0/1을 완료하고 전체 원본 coverage SHA의 동등성을 확인했다. 일부 작업량 감소와 달리 총시간 개선은 관측하지 못했고 producer retained bytes는 늘었다는 결과를 함께 기록했다.
- [x] 수정 파일: 이 문서, 구현 내용: q5의 11-lock/12-draw 제한 probe가 180초 감독 상한으로 종료된 것을 기록했다. 비어 있는 JSON을 정상 확률 0이나 완료 결과로 처리하지 않았다.
- [x] 수정 파일: boundary suffix producer·소비자와 이 문서, 구현 내용: q6의 8-lock boundary 0/1을 완료하고 전체 원본 coverage SHA 일치와 한 pair의 비용 감소를 확인했다. 전체 P7P4는 work-limit 미완료이며 실패시간을 speedup으로 해석하지 않는다.
- [x] 수정 파일: edge closure producer·소비자와 이 문서, 구현 내용: q7의 전체 factor 등록 후 freeze 실패를 단계별 비용과 함께 기록했다. core lib 1,104개 통과와 실제 P7P4 미완료를 구분했다.
- [x] 수정 파일: q8/q9 producer·소비자와 이 문서, 구현 내용: `dcf06bc`의 기본 84개·parallel/local A/B 86개 관측 검증과 release build를 기록했다. P7P4 등록·freeze 완료 뒤 kind union work-limit 실패를 §3.11에 실제 단계 시간과 함께 반영했다.
- [x] 수정 파일: 이 문서와 기존 관측 비교기, 구현 내용: q9의 8-lock 네 arm에서 전체 원래 coverage SHA·확률·완료 gate의 동등성을 확인했다. Rust comparator의 단일 축 비율, collection 호출 감소, 단계 시간과 두 종류의 메모리 계측을 §3.12에 기록하며 순차 단일 표본의 한계를 유지했다.
- [x] 수정 파일: 이 문서, 구현 내용: q9 11-lock의 SIGKILL·빈 JSON·단계 불명과 감독 memory/종료 원인 증거의 한계를 §3.13에 반영했다. timeout 또는 OOM으로 단정하지 않았다.
- [x] 수정 파일: q10 공급 개수 producer·소비자와 이 문서, 구현 내용: `d17c26e`의 92개 focused 검증·release build 및 실제 P7P4의 정책 work-limit 실패를 기록했다. checked/pruned root와 certificate·alias 비용, 부분 메모리·단계 시간을 구분하며 완료 속도 개선을 주장하지 않았다.
- [x] 수정 파일: 독립 `compare_observation.rs`와 이 문서, 구현 내용: 입력 v8의 일곱 번째 supply multiset 축과 다섯 필수 정수 계측을 추가하고 이전 schema는 legacy로 분리했다. `recursion_limit=256` 추가 후 컴파일과 실제 q10 8-lock ABBA 네 arm의 완료·동등성 비교를 마쳤다. 시간 평균과 비율은 Rust 출력에서 인용했다.
- [x] 수정 파일: 이 문서, 구현 내용: q10 8-lock의 정책 상태·행동 검사·총시간과 engine/RSS peak 감소를 §3.15에 기록했다. charged kind-union work는 늘었다는 결과를 함께 유지하며 전체 P7P4 work-limit 해결로 해석하지 않았다.
- [x] 수정 파일: 독립 `compare_observation.rs`, 구현 내용: 입력 v9의 여덟 번째 prefix mask 축과 여섯 필수 정수 계측을 추가하고 비교 출력 v8의 컴파일과 q11 8-lock ABBA의 실제 완료·동등성 비교를 마쳤다. prefix mask work를 board work에 다시 더하지 않는다.
- [x] 수정 파일: prefix mask producer·소비자와 이 문서, 구현 내용: q11 `55ec4b7`의 focused 101개 통과·release 완료와 같은 바이너리 P7P4 mask 1/0의 정책 work-limit 실패를 기록했다. prefix 방문 생략과 helper 비용을 분리했으며 실패시간 speedup이나 단순 `생략 수 − helper work`를 주장하지 않았다. prefix mask 기본은 0이며 제품 미채택이다.
- [x] 수정 파일: 이 문서와 독립 관측 비교기, 구현 내용: q11의 8-lock prefix mask ABBA에서 전체 원래 coverage SHA·확률·완료 gate의 동등성을 확인했다. 평균 비율 1.0077841081246686으로 총시간 개선은 관측하지 못했으며 기본 off를 유지했다. P7P4의 큰 방문 생략량과 8-lock의 작은 생략량·helper 비용 차이를 구분했다.
- [x] 수정 파일: subset import·공개 전 memo와 연결 소비자, 구현 내용: q12 `cd08d4b`·q13 `c3b8adf`를 `abddcccc865472b0cb0c7b4b0ebb7da43dcf984b`에서 연결하고 관측 110개·subset import 5개 focused 검증을 각각 완료했다. q11 보관 바이너리의 실측에는 해당 후속 코드를 포함하지 않는다.
- [x] 수정 파일: 이 문서와 독립 관측 비교기, 구현 내용: 짧은 source SHA를 가진 첫 q13의 출처 gate 실패를 예비 자료로 보존하고 정식 timing에서 제외했다. v10 입력의 두 새 축·필수 계측을 추가한 비교 출력 v9와 전체 SHA metadata를 넣은 q13b release build를 완료했다.
- [x] 수정 파일: 이 문서와 독립 관측 비교기, 구현 내용: q13b의 정식 8-lock 다섯 arm에서 전체 원본 coverage SHA·확률·완료 gate를 확인했다. memo ABBA 평균 비율 1.1088706098916772의 악화와 기본 off 유지, 낮은 raw cache 재사용을 기록했다. subset import의 중앙 kind arena 축소와 비대칭 표본 시간 비교를 분리하고 shard 합산·공존 snapshot의 계측 의미를 명시했다.
- [x] 수정 파일: 이 문서, 구현 내용: q13b P7P4가 77.472324947초 후 공통 work 상한으로 거절된 최종 JSON·stderr·receipt를 확인했다. 전체 등록·freeze 완료와 중앙 arena 축소, 공개 전 memo의 full fallback 및 최종 탐색 미완료를 구분했다. 실패시간 speedup을 주장하지 않는다.
- [x] 수정 파일: q14 존재성 producer·suffix compiler와 연결 소비자, 구현 내용: `188e583b7e59767d3f95d88ad430585d26cf0d75`의 typed 존재성 판정과 명시적 strict-prefix 경로 보존을 통합했다. 테스트 import 오류 수정 후 관측 120개·suffix 3개와 release build를 완료했으며, 두 projection=false 적용 경계를 명시했다. 35,126개 구성 word와 12개 독립 경계 비교를 전칭 증명과 구분했다.
- [x] 수정 파일: 독립 관측 비교기와 이 문서, 구현 내용: v11 입력의 열한 번째 source-existence 축·typed 공급 metadata·여섯 필수 계측·qualified candidate count 진단을 추가했다. 출력 v10의 컴파일과 기존 q13b 다섯 arm의 backward comparison을 완료했다. 후보 집합 해시 검증으로 과장하지 않는다.
- [x] 수정 파일: 이 문서와 독립 관측 비교기, 구현 내용: 실제 v11 source-existence ABBA의 전체 원본 coverage·확률·완료 gate와 accepted 2,597개 보존을 확인했다. 평균 비율 1.0351020300827798의 총시간 개선 미관측을 기록하고 oracle 진단값의 의도적 0을 최종 coverage와 구분했다.
- [x] 수정 파일: 이 문서, 구현 내용: q14 P7P4가 전체 456,459 후보를 등록·freeze한 뒤 33.97099872초에 정책 work 상한으로 종료한 JSON·stderr·receipt를 기록했다. 후보별 oracle 생략과 전체 탐색 미완료를 함께 유지하고 실패시간 비율을 만들지 않았다.
- [x] 수정 파일: 이 문서, 구현 내용: q14 11-lock의 578.629917026초 physical node-limit 실패와 exit 1을 최종 artifact·receipt로 확인했다. 실패 worker의 부분 source/kind 수치와 전체 미완료를 구분하며 timeout·OOM으로 표시하지 않았다.
- [x] 수정 파일: source-qualified prefix compiler와 연결 소비자, 구현 내용: q15 `55167107eee0fa66a880a355b430cb45f5d89979`의 관측 124개·직접 filter 7개 focused 검증 및 release build를 완료했다. 독립 word/filter 회귀를 전칭 증명과 구분한다.
- [x] 수정 파일: 독립 관측 비교기와 이 문서, 구현 내용: 후속 v12 source-filter 축과 네 필수 구조 계측, qualified calls==accepted 및 단일 축 count 진단을 추가했다. 비교 출력 v11의 컴파일, 기존 q14 backward comparison, 실제 q15 네 arm의 완료·동등성·후보 수 검증을 통과했다.
- [x] 수정 파일: 이 문서, 구현 내용: q15의 8-lock source-filter ABBA에서 평균 비율 0.9344308135557023과 구조·정책 계측 감소를 기록했다. P7P4는 전체 등록·freeze 후 35.667625777초에 공통 work 상한으로 미완료이며 실패시간 비율을 만들지 않았다.
- [x] 수정 파일: 이 문서, 구현 내용: q15 11-lock의 578.288882526초 physical node-limit 실패·child exit 1을 최종 JSON·receipt로 확인했다. 전체 worker 생산 종료 후 coordinator merge/import까지 도달한 단계를 q14 worker 실패와 구분하고 부분 snapshot·RSS·미완료 계약을 함께 기록했다.
- [x] 수정 파일: 독립 Rust 작업량 추출기와 이 문서, 구현 내용: q15 11-lock의 worker 집계를 실제 Rust 산출물로 확인하고 producer work 하계 77,863,423의 전제·적용 범위를 명시했다. 현재 producer의 병목과 다른 알고리즘의 가능성을 구분하며 resource 상한을 바꾸지 않는다.
- [x] 수정 파일: 이 문서, 구현 내용: 같은 q15 P7P4의 supply multiset 0 대조가 freeze 후 179,251ms에 SIGKILL로 종료됐음을 receipt와 빈 JSON으로 기록했다. OOM·timeout·speedup을 단정하지 않는다.
- [x] 수정 파일: 독립 관측 비교기와 이 문서, 구현 내용: q16 v13 입력의 multiset-groups 축·아홉 필수 계측·선행 mode 권위·완료 fixture의 생성 여부·단일 축 후보 수 검증을 비교 출력 v12에 추가했다. 컴파일·기존 q15 backward comparison 및 실제 q16 네 arm의 완료·동등성·후보 수 검증을 통과했다.
- [x] 수정 파일: kind-union group index·연결 소비자와 이 문서, 구현 내용: q16 `86aa5b8e255c53636bfacc21a04c4efa706c2759`의 관측 129개·release build를 기록했다. B17의 정확한 root 집합 동치와 구현 회귀를 구분하고, producer 비용을 줄이는 변경이 없어 확인된 병목에 대한 새 근거 없이 11-lock을 반복 실행하지 않은 이유를 명시했다.
- [x] 수정 파일: 이 문서, 구현 내용: q16 8-lock ABBA의 총시간 개선 미관측·group 기본 0 유지와 kind work 감소를 구분했다. P7P4는 첫 초기 관측 완료 기록 뒤 38.250364388초에 공통 work 상한으로 미완료이며 단계별 계측·receipt를 기록했다.
- [x] 수정 파일: 독립 관측 비교기와 이 문서, 구현 내용: q17 v14 branch-bound 축·네 필수 정책 계측·완료 fixture의 attempts gate를 비교 출력 v13에 추가하고 컴파일·기존 q16 backward comparison 및 실제 q17 ABBA 완료·동등성 검증을 통과했다.
- [x] 수정 파일: 관측 정책 branch-bound·연결 소비자와 이 문서, 구현 내용: q17 `7108df03b50df28c8a04eae7dd71f1e2f22dd76d`의 관측 135개·release 및 8-lock ABBA를 완료했다. B18의 정확성 논증·여섯 구현 회귀와 평균 비율 0.411130754380963의 정책 비용 감소를 구분한다. P7P4 group 0/1은 전체 등록·freeze 뒤 정책에서 64M work 상한으로 미완료이며 실패시간 비율을 만들지 않았다.
- [x] 수정 파일: 독립 관측 비교기 소스와 이 문서, 구현 내용: v15 prefix-cones 축·다섯 필수 계측을 추가하고 실제 q18 8-lock ABBA의 완료·원본 coverage 동등성을 확인했다. 총시간 개선 미관측, P7P4 정책 work-limit, 11-lock 중앙 node-limit과 모든 worker의 71,396,217 work를 함께 기록했다.
- [x] 수정 파일: q19/q20/q21 producer·소비자와 이 문서, 구현 내용: q21 관측 166개 통합 검증과 release를 마쳤고 실제 8-lock cone/class 두 ABBA를 별도 비교했다. Bitmap의 8-lock work-limit과 cone/class의 P7P4 비용 악화를 생략하지 않았으며 실패시간을 속도 개선으로 해석하지 않았다.
- [x] 수정 파일: 독립 관측 비교기 소스, 구현 내용: 입력 v17/출력 v16 및 17축 계약을 추가하고 q21 backward comparison을 통과했다. Target-M query와 Full certificate의 활동 gate를 기존 v16과 구분한다.
- [x] 수정 파일: q22 producer·소비자와 이 문서, 구현 내용: 관측 176개·release 및 실제 8-lock cone/bitmap 두 ABBA를 완료하고 원래 coverage를 보존했다. Bitmap 비용 악화와 P7P4 class/bitmap/groups 각 경로의 정책 work-limit을 §3.44–3.46에 기록했다.
- [x] 수정 파일: q23 공유 producer·증분 importer·native 연결과 이 문서, 구현 내용: 서로 다른 관측 179개·importer 5개 검증과 release를 완료했다. 공유 request와 private worker 소유량, 완료 authority, 원래 8M/64M 상한을 유지하며 source/binary SHA와 receipt를 §3.47에 기록했다.
- [x] 수정 파일: 독립 관측 비교기와 이 문서, 구현 내용: 입력 v18/출력 v17/18축의 q23 8-lock shared ABBA에서 전체 원본 coverage·확률·완료 gate를 통과했다. 작은 평균 차이와 각 표본의 engine/RSS peak, candidate digest 미계산 경계를 §3.48에 기록했다.
- [x] 수정 파일: 이 문서, 구현 내용: q23 P7P4 group 0/1의 등록·freeze 완료 뒤 정책 work-limit과 초기 관측 4/5,040·13/5,040의 진척을 §3.49에 기록했다. 11-lock의 생성 중 8M suffix 참조 거절·최초 원인·실제 owner 진단은 §3.50에 기록했다. 실패시간 비율을 만들지 않았다.
- [ ] 수정 파일: 후속 관측 producer·소비자와 이 문서, 구현 내용: 원래 공급·관측 계약의 10/11-lock 완주를 달성한다. q17의 완료된 8-lock 비용 감소·focused 검증과 전체 10/11-lock 완주는 구분한다. 관측된 producer·정책 병목을 함께 다루며 full family가 없는 raw residual의 임의 병합을 허용하지 않는다. 미완료를 정상 확률 0으로 바꾸지 않는다.
- [x] 수정 파일: `queue_observation_policy.rs` 및 관련 physical view, 구현 내용: 실제 공급행동·kind·cells로 공통 deterministic physical tie 계약을 구현했다. 기존 raw child ID의 우연적인 동률 선택과 정확히 같은 selected-ID subset을 보장한다는 주장과, 정책 값·합법적인 성공 행동 집합을 보존한다는 주장을 구분한다. q3 전체 실행의 성공 근거로 선행 간주하지 않는다.

후속 최적화도 미완료·resource refusal을 정상 실패 큐로 바꾸거나 원래 관측 권한보다 먼저 숨은 공급을 공개해서는 안 된다. 필요한 관련 공급이 모두 보이거나 원래 공급에서 확정된 경우에만 oracle과 같은 존재성 평가를 사용한다. 빈 Hold를 처음 채우는 행동은 lock 수보다 한 draw를 더 읽을 수 있으므로 lock 수만으로 공개 완료를 판단하지 않는다. P7 통과를 실제 숨은 선택이 있는 8~11 lock see-7 완주의 대체 근거로 사용하지 않는다.
