# Qnia CP-SAT 전환과 최소 해법 5초 기준 재검토

상태: **참조 실행에서 5초 수준 재현, Clearra 제품 기준은 미충족. 배포 보류.**
기존 HiGHS 비교 및 native/WASM32 RNG 비교는 별도 역사 문서로 보존한다.

## 버전과 비교 계약

- 참조: Qnia `sfinder_wasm` 3.0,
  `03b637730c5b541f4f2934be613498fbe65327fd`.
- 공식 OR-Tools 9.15 기반의 별도 JSPI/WASM 포트. 원격 탐색 서버나 native solver
  프로세스를 호출하지 않는다. 이번에도 기존 Node24.16.0과 배포된 WASM을 사용했다.
  native 빌드·테스트에 대한 Windows 실행 정책은 우회하지 않았다.
- 기본 Auto는 축약 후 cases>=200, candidates>=112, entries>=2200에서 CP-SAT를 선택.
  실제 설정은 numWorkers=2, subsolvers=[max_lp], randomSeed=1,
  addZeroHalfCuts=false, useSatInprocessing=false다.
- 최소 개수는 OPTIMAL 및 objective=bound를 요구한다. 그러나 기본 Fast secondary는
  정확한 K를 유지하면서 품질 선택을 제한할 수 있다. Clearra의 정확한 첫 canonical
  최소 집합 및 후속 전체 동률 페이지 계약과 같지 않다.

사용자가 최소 해법 배포 기준을 **5초 내외**로 변경했다. 고정 입력은
`ctk3_w0kCQBjwwAMPPAD37g` / P7 / 4L / SRS+이며 최소25는 사후 검증용이다.
primary의 25나 이미 성공한 선택 집합을 입력으로 주입하지 않았다. 일반 PC/구축 확률
전체 해법의 합의된 약0.4초 기준 및 패턴 전체 리플레이 해결 후 배포 조건은 유지한다.
이5초 기준을 일반 사용자 탐색의 hard timeout으로 구현하지 않는다.

## 실제 Qnia 기본 실행: 전체 feature 구간

Qnia 기본 PC 물리는 Jstris다. 입력 필드의 모양은 Clearra fixture와 정확히 맞춘
`0x3c0f03c0f`; 과거 upstream 벤치의 좌우 반전 `0xf03c0f03c0`를 사용하지 않았다.
pattern=`*!`, hold=true, save filter 없음, Primary=Auto, exactHumanQuality=Fast.

기본 PC WASM/legal data 초기 준비를 별도로 잰 뒤 동일 solver에서 순차3회 실행했다.
각 primary 호출은 upstream 정책대로 CP-SAT worker/runtime을 새로 만들고 종료한다.
아래 전체 시간은 source 열거부터 최종 Fumen 결과 생성까지이며 GUI transport/렌더는
포함하지 않는다. source hook은 실행 중 메모리에 계측만 추가하고 upstream 파일은
변경하지 않았다. Cargo/다른 에이전트 벤치를 동시에 실행하지 않았지만 일반 사용자
프로그램과 OS 부하까지 통제한 통계 실험은 아니다.

| 구간 (초) | 최초 | 재사용1 | 재사용2 |
| --- | ---: | ---: | ---: |
| 전체 feature | 4.038 | 3.662 | 5.543 |
| 전체 source 열거 | 0.085 | 0.022 | 0.020 |
| coverage/quality 집계 | 0.044 | 0.032 | 0.040 |
| kernelization | 0.023 | 0.012 | 0.011 |
| CP-SAT primary: worker/load/검사/종료 포함 | 3.497 | 3.324 | 5.035 |
| Fast secondary 선택 | 0.339 | 0.236 | 0.374 |

모두 원본 해법246개, 성공5040/5040, OPTIMAL25였다. 모두
`humanQualityExact=false`, `qualityBackend=fast-2x2`였다. 따라서 사용자가 전달한
약5초는 이 환경에서도 실현 가능한 관찰이지만, 정확한 canonical secondary나
브라우저 end-to-end5초 증명은 아니다. 3개 표본의 중앙값은4.038초다.

## 같은 Clearra 행렬의 CP-SAT primary

기존 실제 Clearra 행렬
`ad8af9326bd6a1eaa3c25747f33d6b9c1ed825601c1431c22b5aec636e7563b9`
의246행/5040pattern/79word를 해시 재검증하고 전치했다. 모든 required pattern에
후보가 있는지 검사하며 빈 행을 버리지 않았다. CP-SAT 설정은 위와 동일하다.

| primary만 포함한 구간 | 실행1 | 실행2 | 실행3 |
| --- | ---: | ---: | ---: |
| worker/load/solve/원본 coverage 검사/종료 (초) | 8.358 | 11.290 | 11.545 |
| 최소 개수 및 증명 하한 | 25 / 25 | 25 / 25 | 25 / 25 |

이 구간에는 source, 행렬 전치, kernelization 및 Clearra canonical 선택이 없다.
이전 같은 행렬 HiGHS primary34.304초보다 짧은 관찰이지만, 서로 다른 시점의 실행을
통계적 동일 조건 speedup으로 표시하지 않는다. 이 primary만으로도 아직5초를 넘는다.

### 워커/portfolio 구성 추가 진단

같은 원행렬·ordered kernel·WASM을 사용하는 독립 Node child를 순차 실행했다.
세 조건 모두 numWorkers/subsolvers만 바꾸고 seed/cuts/inprocessing 설정은 위와
같게 유지했다. 각 조건에 공통으로 숫자 subsolver 로그 계측을 추가했으므로, 앞의
Qnia adapter2워커 결과와 워커 수만의 엄밀한 비교는 아니다. 각 조건1회 관찰이다.

| 설정 | API/model/load/solve/검사/종료 (초) | 관측된 full subsolver |
| --- | ---: | --- |
| 1워커 + max_lp | 16.976 | 1개, max_lp1 |
| 11워커 + 기본 portfolio | 9.738 | 8개, 그중 max_lp1; first-solution3 |
| 11워커 + max_lp | 13.214 | 8개 전부 max_lp; first-solution3 |

전부 OPTIMAL25 및 원본5040큐 coverage를 검증했다. 11워커 max_lp는 실제로 같은
전략의 full subsolver8개를 구성했으므로, 워커 수만 예약하고1개만 일한 것으로
설명할 수 없다. 이 관찰에서는 단순한 워커 확장으로5초를 만들지 못했다.
실험의60초 parent deadline은 미증명 진단 중단용이며 제품 hard cap이 아니다.
실행된8,038,746-byte CP-SAT WASM의 SHA256은
`7482593df2d8fde213d17abd14a47d0def30401ab96d3afa6e0b5d14042c0030`이다.

### 행렬 차이와 kernelizer 교차 확인

최신 Qnia의 각 필드와 구체 큐를 Clearra canonical 순서에 맞추어 직접 대조했다.
246개 필드 identity는 전부 일치하지만4개 필드,452개 큐에 걸쳐496개 incidence가
다르다(전체1,239,840개 중). 모두 Clearra 쪽 추가 hit였다. Jstris/SRS+ 차이를
유지한 비교이며, 이496개의 차이만이 시간 차이의 유일한 원인이라고 단정하지 않는다.

| 입력 | 남은 제약 | 남은 후보 | incidence |
| --- | ---: | ---: | ---: |
| Clearra 원본 행렬 | 1456 | 158 | 16078 |
| Qnia 행렬, Clearra 순서로 정렬 | 1389 | 158 | 15128 |
| Qnia 행렬, Qnia 원래 순서 | 1389 | 158 | 15128 |

각 입력에서 Qnia JS kernelizer와 Rust/WASM kernelizer가 **ordered cases와
solution IDs까지 동일**했다. 따라서 이 실험에서 kernel 크기 차이를 JS/Rust 전처리
구현 차이로 설명할 수 없다. 조합 최적화의 증명 비용이 coverage 변화량에 선형적으로
비례한다고 가정하거나, 킥테이블 차이를 속도 맞추기 위해 제거해서는 안 된다.

## Clearra 적용 방향

1. 제품 입력·coverage·canonical 순서·전체 동률·취소/메모리 권한은 CLI 공통 App/
   Rust 도메인에 유지한다. GUI 전용 CP-SAT 결과 판정을 만들지 않는다.
2. 첫 작은 실험은 기존 정수 dual을 이용한 조건부 후보 제거다. 현재 uncovered
   가중치N, 모든 eligible행 load 상한D, 남은 선택수k일 때
   `N-load(r) > (k-1)*D`인 pivot후보r는 포함될 수 없다. checked 정수 산술만
   prune 권한을 가지며 invalid/overflow는 이 optional prune을 생략한다. 초기
   certificate 순회를 재사용하고 임계치가0이하이면 후보 순회를 생략한다.
3. 실패한 부정증명에서도 유효한 proposal 정보를 warm-start로 재사용하는 경계와
   canonical 질의 사이의 반복 준비 비용을 다음 후보로 둔다. 바뀐 행렬/index에서
   오래된 certificate·memo를 무검증 재사용하지 않는다.
4. CP-SAT 전체를 새로 작성하는 것은 작은 수정이 아니다. SAT 학습, PB propagation,
   LP simplex, cuts, presolve가 결합된 구조다. 위 특화 구현을 먼저 계측하되,
   그 결과로5초를 충족하지 못하면 검증된 CP-SAT를 공통 백엔드 어댑터에 넣는 선택을
   유지한다. 일부 아이디어 구현을 Google CP-SAT와 동등하다고 부르지 않는다.
5. 벤더링 시에는 원본/포트/전이 의존성 고지, 재현 빌드, JSPI/SAB/COOP-COEP
   미지원 경로, CP-SAT 상태, 취소 및 source/Proto/solver/pthread/JS의 whole-live
   소유권을 모두 다룬다. `max_memory_in_mb`는 Clearra 사전 메모리 guard의 대체가
   아니며, 외부n워커마다 내부n워커를 만드는 중첩 병렬화도 금지한다.

특화 행 제거 구현과6개 회귀 테스트 소스를 후보 브랜치에 반영했다. 등호 유지,
작은 행렬 완전열거, word 경계, overflow/형상 오류, 전체 canonical 동률, 실제
pivot/undo/witness/취소 경계를 포함한다. 필터는 현재 노드의 residual MP **이후**
위치하며 다음 child를 줄이는 실험이다. MP 이전 종료나 새 LP solver 구현을
달성했다고 주장하지 않는다. 별도 diagnostic setter로 동일 바이너리 A/B가 가능하며
생산 빌드에는 setter가 없다. 포맷·정적 교차 검토만 마쳤고 제품 컴파일·정확성 및
성능 통과는 아직 미검증이다. 현재4194의 검증된8b8 WASM에서 최소 GUI는40.5/43.0/41.7초였고,
이번 Qnia 비교가4194를 교체하거나 해당 성능을 개선한 것은 아니다.

## 재현 자료

- `_local/qnia-cpsat-common-matrix-benchmark.mjs`
- `_local/qnia-cpsat-stage-benchmark.mjs`
- `_local/qnia-cpsat-kernel-comparison.mjs`
- `_local/qnia-cpsat-parameter-probe.mjs`
- `_local/qnia-benchmark-results/1788705095098-cpsat-common-matrix.json`
- `_local/qnia-benchmark-results/1788705217944-cpsat-feature-clearra-field.json`
- `_local/qnia-benchmark-results/1788705445047-cpsat-kernel-comparison.json`
- `_local/qnia-benchmark-results/1788706119580-7b04c924-ebb8-4bde-8551-614f81d2a6fe-cpsat-parameter-probe.json`

위 파일은 local diagnostic 증거이며 릴리스 acceptance authority가 아니다. 외부
소스와 WASM은 별도 `_local/qnia-cpsat-reference-03b6377`에만 있고 제품 의존성이나
라이선스 목록에 조용히 추가하지 않았다.

## 일차 출처

- [Qnia 고정 커밋](https://github.com/Qnia28/sfinder_wasm/commit/03b637730c5b541f4f2934be613498fbe65327fd)
- [Qnia CP-SAT 설정과 검증](https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/src/ortools-min-cover.mjs)
- [Qnia 실제 모델](https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/src/ortools-primary-worker.mjs)
- [Qnia Fast secondary](https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/src/min-cover-adaptive.mjs)
- [Qnia 포트·지원·고지](https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/ORTOOLS_INTEGRATION_AND_LICENSE.md)
- [Google CP-SAT 구조](https://github.com/google/or-tools/blob/stable/ortools/sat/README.md)
- [Google LP propagator와 정수 재검증](https://github.com/google/or-tools/blob/stable/ortools/sat/linear_programming_constraint.h)
- [Google solver 상태](https://developers.google.com/optimization/cp/cp_solver)
- [Google solver 자원/병렬 설정](https://github.com/google/or-tools/blob/stable/ortools/sat/sat_parameters.proto)
