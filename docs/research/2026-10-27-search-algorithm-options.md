# 탐색 알고리즘 비교와 후속 연구 후보

이 문서는 아이디어 제안이다. 아래 후보를 제품 실행 경로에 연결하거나, 성능이
개선되었다고 판단하지 않는다. 이번 작업에서는 속도를 계측하지 않는다. 기존 PC4
Tablebase 계획과 자료 자격은 변경하지 않는다.

## 현재 경로와 sfinder의 관계

Clearra의 PC/Build Geometry는 inverse lock-clear(ILC) 실현을 기반으로 후보를
구성하고, BuildUp과 공급 검증을 거쳐 공통 reducer에 전달한다. Geometry에는 이미
잔여 상태 memo, 최소 지지 셀 pivot, 열·연결 성분 기반 판정과 인증된 separator
합성이 있다. BuildUp은 operation 부분집합에 따른 원행 삭제와 압축 물리 보드를
묶는다. 삭제행 의존 순환에 대한 필요조건 검사도 이미 존재한다. 이들을 새 제안으로
중복 구현하지 않는다.

sfinder `path`를 단순한 전방 탐색이라고 부르는 것은 정확하지 않다. sfinder의
[separable mino 생성기](https://github.com/knewjade/solution-finder/blob/0e7c935a5399159a3d9c42fb8721e3c6842ae17d/src/main/java/searcher/pack/separable_mino/AllSeparableMinoFactory.java#L69-L101)도
원래 행과 삭제 행을 표현한다. 이후 [packing과 BuildUp](https://github.com/knewjade/solution-finder/blob/0e7c935a5399159a3d9c42fb8721e3c6842ae17d/src/main/java/entry/path/PathCore.java#L46-L88)을
결합한다. 그 [packing 폭 선택](https://github.com/knewjade/solution-finder/blob/0e7c935a5399159a3d9c42fb8721e3c6842ae17d/src/main/java/entry/path/PathEntryPoint.java#L220-L260)은
4L 이하 3열, 그 이상 2열이다. 두 제품의 주된 비교점은 ILC 유무만이 아니라
상태의 분할·결합, 후보 물질화와 공급·도달성 검증의 순서다. 원본 sfinder
`path`의 [높이 한계는 10L](https://github.com/knewjade/solution-finder/blob/0e7c935a5399159a3d9c42fb8721e3c6842ae17d/src/main/java/entry/Verify.java#L23-L25)이므로
11~24L의 독립 정답 oracle로 취급하지 않는다.

## 연구 후보

1. **열 경계 frontier DP / meet-join.** 이미 분리된 컴포넌트만 합성하는 현재
   separator와 달리, 작은 폭의 경계를 가로지르는 placement 상태를 명시해 부분
   결과를 재사용하는 가설이다. 원행·삭제 상태, 초기 필드, 조각 inventory 및
   경계 미노를 모두 보존해야 한다. 높이가 증가하면 상태가 폭증할 수 있다.
2. **물리 필드 기반 전방 DP/BFS.** 고정 큐·존재 여부처럼 공급 제약이 강한
   요청에서 미리 전체 기하 family를 구성하지 않을 수 있다. 상태에는 물리 보드,
   원행 frame, 줄 삭제, kick profile, 다음 큐와 hold가 필요하다. 전체 해법·점수·
   replay에서는 합쳐지는 경로의 provenance와 CountAll multiplicity를 별도 DAG에
   보존해야 한다. 존재 여부에서만 검증된 병합을 전체 열거에 확장하지 않는다.
3. **목표 측 ILC와 시작 측 전방 탐색의 양방향 결합.** 중간 상태의 원행·삭제,
   큐/hold 및 필요한 증거를 완전히 결박한 뒤에만 접합한다. ordered first-success
   kick을 단순 역전하지 않고 원래 전방 전이로 재검증한다. 불완전한 접합 실패는
   UNSAT가 아니다.
4. **Geometry family × BuildOrder × 공급 언어의 지연 symbolic product.** 각
   단계에 이미 있는 DAG·intern·memo를 다시 만드는 것이 아니라 단계 사이의 중간
   family 물질화를 줄이는 가설이다. canonical 순서, 모든 경로의 중복 의미,
   메모리 상한, 취소와 미완결 상태를 유지해야 한다.
5. **최소 해법의 fixed-K incremental assumptions와 reasoned SAT/PB.** 기존
   [minimum 연구](minimum-parallel-overhead-and-algorithm-plan-2026-09-08.md)의
   B1/B2 후보를 재사용한다. 최소 크기 증명과 최초 canonical 집합 선택은 별도
   계약이며, 다른 범위에서 학습된 제약의 재사용·정수 overflow·동률 누락을
   방지해야 한다. CP-SAT의 일부 아이디어를 도입했다고 전체 solver와 동일한
   정확성 또는 성능을 주장하지 않는다.

## 후속 검증 조건

각 후보의 적용 범위를 먼저 고정하고 기존 exact 경로와 전체 candidate identity,
coverage, canonical 순서, replay, score/spin, queue/hold 및 오류를 대조한다.
초기 필드와 1~24L, 다섯 kick profile, 고정 큐와 pattern, 1·2·11 worker,
취소·자원 제한에 따른 미완결 결과를 각각 포함한다. 분산 분할은 누락·중복 없는
소유권과 결정적인 병합부터 증명한다. 범위가 좁은 독립 oracle은 그 지원 범위를
넘겨 일반화하지 않는다. 시간과 메모리의 이득은 이후 별도의 계측 전까지 미확인이다.
