# Qnia 최소 개수 증명 편차: 동일 입력의 병렬/알고리즘 A/B

## 결론과 적용 범위

동일 P7 행렬, 동일 seed에서도 비동기 CP-SAT portfolio의 진행 경로가 달라졌다.
결정적 interleave 실행에서는 네 번 모두 동일한 deterministic work와 첫 subsolver
카운터가 나왔지만, 기본 4.90~5.38초보다 약 3배 느린 15.39~15.64초였다.
**병렬 순서를 고정하는 것은 편차 원인의 분리 도구이지 채택할 성능 개선이 아니다.**

LP 중심 backend를 core 중심으로 바꾸면 약 41초가 필요했다. Clearra의 개선 방향을
단순한 SAT 분기 추가나 worker 수 조정으로 축소하지 않는다. 강한 하한의 증명,
가정 범위가 있는 학습, 검증된 incumbent의 조기 공유가 별도 알고리즘 축이다.

아래는 2026-09-07에 실행 완료한 로컬 WASM 비교의 기록이다. Qnia의 임의 optimal
witness/K 증명과 Clearra의 **원본 ID 기준 첫 canonical 최소 집합**은 서로 다른
종료 조건이다. 이 수치를 Clearra GUI 시간이나 S23 Ultra 실기 계측으로 표현하지 않는다.
외부 구현·raw matrix·solver 바이너리·실행기는 `_local/`에만 있고 배포에서 제외된다.

## 입력과 실행 바인딩

- Qnia revision: `03b637730c5b541f4f2934be613498fbe65327fd`.
- 입력: `ctk3_w0kCQBjwwAMPPAD37g`, P7, 4L, hold, Jstris 180.
- 원본: 후보 246개 / 큐 5,040개. raw bytes 및 후보/큐 순서 바인딩을 검증했다.
- Matrix SHA256: `63de33e1d86077c179a38f6311df893ba9abcc13c9b18fa433384f5961eeee91`.
- Qnia kernel: 제약 1,389개 / 후보 158개 / incidence 15,128개.
- Kernel JSON SHA256: `52bd689ef0c8a63b566491502f480814d85710d849fc5e3d8a249c983ef51f64`.
- 기본값: 2 workers, `max_lp`, seed 1, zero-half cuts false,
  SAT inprocessing false. 모든 변형에 동일하게 search log를 활성화했다.
- 실행 순서: `A D D A S S C C A D D A`. 동시에 하나의 비교만 실행했다.
- 시간: CP-SAT의 `wallTime`, module/worker load, model 준비, 종료 시간은 별도.
  이름에 native가 있는 solver 내부 clock이지만 실행체는 **Node의 WASM**이다.
  이 호스트에서 Clearra native Rust/Cargo 테스트를 실행한 것이 아니다.
- 12개 모두 OPTIMAL, K=25, proof bound=25. 선택한 후보를 원본 5,040개 큐에
  재검증했다. 알려진 25는 출력 사후 조건으로만 사용했다.

## 모든 유효 표본

| 변형 | 실제 solver 시간, ms | 판정 |
| --- | --- | --- |
| A: 공개 기본 2 workers / max_lp | 5,379 / 5,214 / 5,206 / 4,897 | 중앙값 5,210; 이번 표본 범위 4.90~5.38초 |
| D: interleaveSearch, batch 16 | 15,548 / 15,636 / 15,607 / 15,389 | 순서 재현성 확인, 성능 후보로는 제외 |
| S: 1 worker / max_lp | 5,281 / 5,250 | 이번 입력은 2 workers의 단순 선형 가속 대상이 아님 |
| C: 2 workers / primary core, 첫 subsolver LP=0 | 41,307 / 40,861 | 약 8배 느려 채택하지 않음 |

기본 A의 deterministic time은 12.5325~13.3542로 달랐다. D의 네 표본은 모두
29.4087485234226이며 첫 subsolver의 branches=2,170, conflicts=1,404,
LP iterations=116,014, restarts=4가 일치했다. S도 두 번의 경로가 일치했다.
C에서는 첫 subsolver가 약 101만 branches / 77만 conflicts를 처리했고 LP
iterations는 0이었다. **이 카운터는 전체 portfolio 합계가 아니라 첫 subsolver의
카운터**이므로 서로 다른 알고리즘의 총 연산량처럼 합산하거나 해석하지 않는다.

## 구간별로 드러난 차이

| 변형 | 25개 witness 발견 | 최적성 증명 완료 | 중요한 관측 |
| --- | --- | --- | --- |
| A | 1.15~1.36초 | 4.90~5.38초 | 나머지 약 3.7~4.0초는 24개 이하가 불가능함을 증명하는 비용 |
| D | 1.07~1.11초 | 15.39~15.64초 | witness가 늦어서가 아니라 lower bound 향상이 늦음 |
| C | 1.25~1.28초 | 40.86~41.31초 | 빠른 feasible 해가 빠른 최적성 증명을 보장하지 않음 |

A는 lower bound 21을 0.28~0.42초, 22를 3.64~4.13초에 얻었다.
D에서는 22에 도달하는 시점부터 약 13.4초였다. C가 bound 24를 약 8.6~8.8초에
얻어도 마지막 proof가 약 41초까지 남았다. 즉 **큰 tail은 작업 수가 적어져서만
발생하는 것이 아니라 어려운 부정 증명 자체에서 발생할 수 있다.**

기본 로그의 최종 해는 `rnd_cst_lns` 또는 `rins_lp_lns` 계열에서 나왔고,
같은 seed라도 witness/하한 전달 순서와 deterministic work가 달랐다. 이를
비동기 하위 탐색 간 상호작용의 근거로 삼는다. 다만 이번에 재현한 시간 범위는
4.90~5.38초이다. 과거 4~10초 전체 편차를 이 한 가지 원인으로 모두 설명했다고
주장하지 않는다. 과거 표본의 host 경쟁·발열·브라우저 상태는 측정되지 않았다.

## Clearra 핫픽스에 반영한 결정

1. 알고리즘 G와 스케줄링 M을 독립 축으로 비교했다. 워커를 빨리 켰다는 사실만으로
   수학적 탐색이 개선됐다고 간주하지 않는다.
2. 전역 positive-only warm repair를 삭제한 M1은 36.15→82.02초로 악화했다.
   따라서 검증된 witness를 빨리 공유하는 역할을 유지한다.
3. M2는 그 repair를 보존하면서 n-1 설정의 원격 proof 작업과 겹쳐 실행한다.
   controller는 한 advisory step마다 host로 반환한다. 신규 cube receipt를
   위조하거나 warm witness를 그 receipt로 처리하지 않는다.
4. 독립 구현한 G는 후보당 incidence가 최대 2인 부분 제약들의 연결 성분별
   `ceil(|C|/2)`를 합하는 정수 하한이다. 두 odd cycle의 LP gap을 강화할 수
   있지만 P7 첫 실측은 36.15→40.22초였으므로 기본 비활성 상태를 유지한다.
5. 더 강한 LP/PB 설명 기반 학습과 fixed-K 질의 간 가정 범위 재사용은 아직
   후속 후보이다. 전체 CP-SAT를 재구현했다거나 3초 목표를 달성했다고 하지 않는다.

Clearra 실제 브라우저 표본, 오류와 당시 reported 7 설정(모바일 실기 아님)은
`minimum-browser-hotfix-ab-2026-09-08.md`에 별도로 기록한다.
사용자의 실제 의도는 8 logical / 7 compute로 정정되었으며, 모바일 추가 측정은
중단했다. 기존 표본의 입력을 소급 변경하지 않는다.

## 재현 자료와 원 출처

- 로컬 실행기: `_local/research/qnia-variance-ab-20260907.mjs`.
- 로컬 원시 보고서: `_local/reports/qnia-variance-ab-1788758327502.json`.
- [Qnia 기본 CP-SAT 정책](https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/src/ortools-min-cover.mjs),
  [Qnia model과 실행 경계](https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/src/ortools-primary-worker.mjs),
  [Qnia kernel과 HiGHS](https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/src/highs-cardinality.mjs).
- [OR-Tools SAT/정수/LP 구성](https://github.com/google/or-tools/blob/stable/ortools/sat/README.md),
  [CP-SAT 상태 의미](https://developers.google.com/optimization/cp/cp_solver).

이 문서는 진단 결과이지 릴리스 승인 또는 Cloud/모바일 실기 동등성 증거가 아니다.
