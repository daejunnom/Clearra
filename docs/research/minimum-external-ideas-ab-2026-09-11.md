# 외부 대화의 Hitting Set / Lagrangian / packing A/B

이 문서는 2026-09-10에 시작한 로컬 실험의 기록이다. 원본 요청은
`C:/Users/강민수/Downloads/외부대화.md`의 아이디어를 A/B하는 것이다.
첨부 대화의 권고는 실험 후보로 다루며, 그 안의 Qnia 재측정 권고는 실행하지 않는다.
Qnia 공개 minimals의 3~5초는 사용자가 확인한 실제 GUI 계측값이다.

## 비교 계약

- 작업 브랜치: `codex/v0.8.0-hotfix-minimum-algorithm-ab`, 시작 commit `3333586`.
- 제품 기준: 기존 최속 N32 조합. 저 fanout의 기존 TF 정책도 그대로 유지한다.
- P7, `ctk3_w0kCQBjwwAMPPAD37g`, 4L, empty hold, Jstris 180.
- 원본 246 후보 / 5,040 큐. 행렬 SHA-256:
  `63de33e1d86077c179a38f6311df893ba9abcc13c9b18fa433384f5961eeee91`.
- 기존 진단 행렬의 내용 해시, 후보 ID binding, 큐 binding을 모두 재검증했다.
  후보는 원본 normalized ID 순서이며 25는 사후 검증값이다. 정답 hint로 전달하지 않는다.
- 행렬 단계는 동일한 단일 compute worker의 WASM `exact minimum` 호출이다.
  parsing / transpose / engine은 포함하지만 WASM load / worker startup / 제품 geometry /
  canonical 선택 / paint는 제외한다. N32의 flag 761을 선택하되, 이 단계에서는
  병렬 dispatcher나 canonical probe 자체가 실행되지 않는다.
- 제품 전체 시간과 행렬 단계 시간을 섞지 않는다. 기본 batch count 3, fresh workers,
  serial batches, profiling off. 기한 초과는 censored 결과이며 최적 증명이나 UNSAT가 아니다.
- CPU는 Intel Core 5 210H, OS가 보고한 12 logical processors, RAM 16,755,945,472 bytes.
  다른 PC의 실제 시간이나 worker 수에 비례하는 CPU 작업량을 추정하지 않는다.
- Git 추적 전부터 수정되어 있던 workflow / release-plan은 내용 해시로 보존 확인했다.

## 이미 있던 것과 이번에 바꾸는 것

Clearra 자체가 전용 exact cover 분기 탐색이다. 단순히 Set Cover라는 이름을 붙이는
것은 신규 알고리즘 구현이 아니다. 기존 코드에는 singleton / dominance reduction,
disjoint-support packing과 sum-over-packing, Mirror-Prox의 분수 제안과 checked-u128
인증, residual warm seed가 있다.

이번 후보는 다음처럼 분리했다.

| 후보 | 이번 실험의 실체 | 기존 경로와 다른 점 |
| --- | --- | --- |
| A | `Felerius/findminhs` 실제 Rust 구현을 독립 WASM으로 실행 | undo 자료구조, vertex include/exclude 분기, 효율·패킹 하한을 가진 별도 엔진 |
| B | 직접 Lagrangian subgradient를 Clearra residual B&B에 통합 | exp/softmax가 없는 직접 objective / gradient / integer penalty 인증 |
| B+기존 | L64 prepass 뒤 기존 Mirror-Prox, L200으로 residual 제안 대체 | 짧은 보조 경로와 대체 경로를 각각 비교; root dual은 동일 |
| C | 큐의 disjoint-support graph와 color-bound clique search | 기존 greedy packing보다 강한 clique 증거를 제한된 시간 안에 탐색 |

외부 엔진 소스는 `9c0b8272c7e133da5770a4c2cfb095eb48f53ccb`로 고정했다.
`Instant`만 WASM host의 monotonic clock으로 연결하고 solver 로직은 그대로 사용했다.
MIT license와 원본/adapter 파일 해시를 로컬 실험에 보존했다. 제품 코드에 외부 엔진을
복사하거나 의존성으로 추가하지 않았다. 원본 efficiency bound에는 부동소수점 epsilon
rounding이 있으므로, 이 구현을 그대로 제품의 checked-integer proof authority로
승격할 수는 없다.

## 25를 root 하한만으로 증명할 수 없는 이유

원본 행렬의 LP를 한 번 계산한 뒤, 얻은 **primal fractional cover**를 정수 가중치로
변환하고 부족한 제약만 양의 가중치로 보정했다. 이후 원본 5,040개 제약 전부와 각
변수의 `[0,1]` 범위를 BigInt로 재검증했다.

- 분모: `1,000,000,000`.
- 분자 합: `20,932,397,960`.
- 인증된 유효 분수 덮개 비용: **20.93239796**.
- LP solver가 보고한 근삿값: 20.9323854895539. 이 근삿값 자체가 인증 근거는 아니다.

유효 primal fractional cover가 있으므로 weak duality에 의해, 추가 정수 cut 없는
Lagrangian/LP dual 값은 20.93239796보다 클 수 없다. 정수 올림 하한도 최대 21이다.
또한 disjoint-support clique의 크기는 정수이고 같은 fractional cover 비용 이하이므로
**최대 20**이다. 따라서 25개짜리 덮개와 25-clique / 기본 LP dual을 만나게 하는 root
증명 경로는 이 원본 행렬에서 성립하지 않는다. 더 많은 반복이나 clique 검색으로 이
한계를 넘을 수 없다. 분기 후 residual 문제나 유효 정수 cut은 별도의 가능성이다.

이 계산은 Qnia GUI나 CP-SAT의 신규 성능 측정이 아니다. 저장된 행렬의 구조를 검증하는
한 번의 LP 계산이며, 정수 최소 해법을 찾은 결과로 사용하지 않는다.

## 정확성 확인

- 외부 엔진 5개 policy, 독립 Lagrangian, clique, Clearra 기준을 96개 작은 행렬의
  모든 후보 부분집합을 직접 열거한 독립 정답과 비교했다.
- 실제 변경한 residual Lagrangian method를 그대로 사용한 7,130개 검사에서, 얻은
  하한이 brute-force residual optimum을 넘지 않았다. 512 seeds, 64/200 steps,
  제외 후보, 이미 덮인 큐, 63/64/128 경계와 advisory seed를 바꿨다.
- 실험의 모든 prune은 `sum(weights) - sum(max(0, row_load - SCALE))`를 checked-u128로
  재계산한 후에만 가능하다. float objective, 제안값, 시간 제한은 proof authority가 아니다.
- 이미 확보한 governed buffer를 재사용한다. proof-wide iteration budget은 두 제안
  방법을 합쳐서 소모하며, 기존 root certificate와 원본 candidate ID를 변경하지 않는다.

## 결과

주 비교 WASM SHA-256은
`c728013339ec63497b5304b97733cdc0a34aad9eb69f82dad5f41a3645bffd37`이며,
동일 서버 session `84be8882-c902-445e-a364-ffa4ea959b3f`에서 측정했다.
아래 시간은 `api.run()` 호출의 외부 측정값이며, 완료 표본은 각 3회다.

| 구성 | flag / variant | 중앙값 초 | 범위 초 | 기준 대비 / 판정 |
| --- | ---: | ---: | --- | --- |
| 기존 Clearra 기준 | 761 | 26.512 | 26.489–26.525 | 기준 |
| L64 prepass + Mirror-Prox | 2809 | 27.550 | 27.495–27.650 | 약 3.9% 느림 |
| L200 residual 대체 | 4857 | 31.576 | 31.566–31.644 | 약 19.1% 느림 |
| L200 대체 + T off | 4825 | 26.564 | 26.533–26.593 | 약 0.2% 느림; 우위 없음 |
| L64 prepass + Mirror-Prox + T off | 2777 | 미완료 | watchdog 91.020 | 90초 예산, 1회 후 중복 배치 생략 |
| findminhs 기본 | variant 0 | 미완료 | watchdog 61.015 | 60초 예산, 1회 후 중복 배치 생략 |
| findminhs packing local search + sum-degree | variant 3 | 미완료 | watchdog 61.019 | 60초 예산, 1회 후 중복 배치 생략 |

T는 기존 distinct dual capacity 하한이다. T를 끄는 조합은 L200의 회귀를 거의
상쇄했지만 기존 기준을 넘지 못했고, L64 hybrid에서는 큰 회귀를 보였다. 따라서
단독 결과만으로 조합 결과를 예측할 수 없다는 사용자 지적을 실제 교차 비교에 반영했다.
이 구현의 hybrid는 두 방법이 기존 proof-wide 2,000,000 iteration budget을 공유한다.
budget을 각각 독립시킨 정책이나 full DYNSGRAD 구현의 속도까지 검증한 것은 아니다.

L64/T-off 기한 초과 기록 `017-ideas-clearra-1789052923093.json`은 초기 GUI가 timeout에
응답 flag를 저장하지 않아 flag 필드가 없다. 실행 전 UI 선택 2777과 그 표본의
`N32 + L64 prepass, distinct-capacity T disabled` notes를 함께 근거로 식별한다.
원시 기록은 수정하지 않았고, 이후 GUI는 입력 flag도 timeout과 함께 기록하도록 수정했다.

초기 별도 WASM `d5497c1f5b4b93b5e8eee51d1b1df5b118754da71678deaa1d0e4a9ce0f9f033`의
Clearra 기준 중앙값은 52.114초였다. 이를 새 WASM L200의 31.576초와 비교하면
거짓 개선으로 보일 수 있다. 같은 새 WASM에서 기준을 다시 실행해 26.512초를 확인했고,
**52.114→31.576초를 알고리즘 개선율로 사용하지 않는다**. 빌드 사이 차이의 원인을
CPU 전력/온도/백그라운드 부하나 코드 배치 중 하나로 단정하지 않는다.

| 보조 증거 탐색 | n | 중앙값 초 | 얻은 증거 | 해석 |
| --- | ---: | ---: | --- | --- |
| 직접 root Lagrangian, 2,000 iterations | 3 | 1.759 | 인증 하한 21, greedy 상한 28 | 최소 25 증명을 끝내지 못함 |
| disjoint-support clique, search budget 1초 | 3 | 1.221 | 크기 18의 검증된 clique | 최대 clique 증명은 미완료; 기존 LP 계열 하한보다 약함 |

clique는 원본 후보 246개를 유지한 채 중복/포함 관계의 큐 제약만 줄였으므로
그래프는 1,602 정점 / 723,606 간선이다. 외부 대화의 158후보/1,389제약 kernel과
다른 준비 단계다. graph 준비 190.7–205.4ms와 약 1초 search를 모두 시간에 포함했다.
greedy clique 16에서 18로 증가했으며, 원본 supporter가 서로 겹치지 않음을 별도로
검사했다. root Lagrangian의 floating best는 약 20.3072였지만 실제 반환한 정수 하한은
독립 BigInt 인증을 통과한 21이다.

별도로 기존 ordinary D의 실제 제품 경로를 11 workers / batch count 3으로 확인했다.
이는 저장 행렬 실험과 측정 종점이 다르며, 위 표와 나눗셈한 speedup으로 해석하지 않는다.

| 실제 제품 경로 | n | 첫 canonical paint 중앙값 | 평균 | 범위 |
| --- | ---: | ---: | ---: | --- |
| 기존 ordinary D, 11 workers | 3 | 16.953초 | 16.971초 | 16.629–17.330초 |

세 결과 모두 K=25, 렌더링 25 fields, known alternatives=1, total=null,
enumeration_complete=false였다. canonical members JSON SHA-256은 기존 기준과 같은
`ca0c7b428c21ecec9728765d1d89485374af93620c14c1ffa050d02e7d1225ce`다.
프로파일링은 꺼져 있고 모든 표본은 visible이었다. 전체 기록은 완료 24개 / censored 4개다.
제품 WASM은 수정하지 않은 기존 ordinary artifact
`38a2bead11346bd74e61a1aa0d7a161d6d7f49851ee5f7f704dd8fc96b08b103`을 사용했다.

## 채택 판단과 남은 범위

측정한 새 조합에서 기존 최속 기준을 이기는 결과를 얻지 못했다. **일반 제품의 기본
정책은 변경하지 않고, Lagrangian은 `minimum-hotfix-ab` 실험 feature에서만 선택 가능하게
유지한다.** 기존 N32 / low-fanout TF와 정확한 원본 ID canonical 계약을 보존한다.
제품 전체의 후보 WASM 빌드/배포로 확대하지 않는다.

외부 문서의 low-treewidth DP, MaxSAT backend, full DYNSGRAD, 별도 proof worker의
공유 학습/상하한 portfolio는 이번에 구현하거나 측정하지 않았다. 이 결과는 해당
알고리즘 전체의 가능성을 부정하지 않는다. 이번에 확인한 구체적 개선 방향은 기본
LP의 반복 속도만 높이기보다 **정수 gap을 줄이는 유효 cut / conflict learning**과,
기존 문서의 **canonical query 사이에서 증명·준비 상태를 재사용하는 경로**다.
추가 relaxation을 도입할 때는 원본 ID canonical / lazy tie semantics를 별도로 보존해야 한다.

## 로컬 재현 경로

원래 checkout의 `_local/research/external-ideas-20260910/`에 private GUI, worker,
실험 WASM, 행렬, upstream adapter와 hash provenance가 있다.
`node _local/research/serve-external-ideas-20260910.mjs`는 localhost 4195에만 바인딩하고
45분 lease 후 종료한다. 자동 실행이나 외부 업로드는 없다.

원시 자료는 원래 checkout의 `_local/reports/external-ideas-20260910/`에 저장한다.
LP의 exact primal certificate와 property 결과도 같은 `_local/reports/` 아래 별도 JSON이다.
이 자료는 로컬 실험 근거이며 릴리즈 승인 또는 배포 완료 근거가 아니다.

독립 실행 묶음은 `_local/benchmark-portable/external-ideas-20260911/`와 그 ZIP이다.
`node serve.mjs --verify-only`는 동결된 GUI / WASM / 행렬 해시를 검증하고,
`node serve.mjs`는 4195에만 서비스를 제공한다. `/ideas/`가 이번 실험 GUI이고,
`/D/`는 기존 ordinary 제품 GUI다. 다른 PC에서도 실제 CPU/RAM/browser 정보를 기록한다.
모든 측정이 끝난 후 포장하며 실행 중 benchmark와 컴파일/압축을 겹치지 않았다.

## 참고한 원문

- [An Efficient Branch-and-Bound Solver for Hitting Set](https://arxiv.org/abs/2110.11697).
- [findminhs 고정 소스](https://github.com/Felerius/findminhs/tree/9c0b8272c7e133da5770a4c2cfb095eb48f53ccb).
- [OR-Tools v9.15 direct Lagrangian 구현](https://github.com/google/or-tools/blob/v9.15/ortools/set_cover/set_cover_lagrangian.cc).
