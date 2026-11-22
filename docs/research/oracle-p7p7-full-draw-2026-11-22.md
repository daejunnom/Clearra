# CTK3 목표 영역의 oracle P7P7 full-draw

작성일: 2026-11-22. 선행 로컬 tree는 `15345623ab3a3c1172f87bc7cd65d8b81312f310`이며,
동일 tree의 게시 기준은 `d4ad64c34f6b4ce38eea13cb5dd8f51f7dfc83f0`이다.
이전 작업 branch는 `codex/oracle-p7p7-20261122`이며, 복구 후 작업 branch는
`codex/oracle-p7p7-resumed-20261122`다. 복구 기준과 계승 상태는 7절에 기록한다.

## 1. 변경된 목표와 입력

사용자의 새 우선순위는 미사용 WebGPU 실행 구현 제거, 임의 생성 한도의 제품
경로 분리, 아래 보드의 oracle P7P7 full-draw 전체 탐색을 수 분 안에 완료하는 것이다.
11미노 see-7 완주는 이번 작업 목표에서 제외한다. 이전 see-7의 완료·실패 기록과
증명은 역사 자료로 보존하며 새 목표의 성능으로 합산하지 않는다.

입력은
`ctk3_w0kCoLsawgRDCgEBkPgHQBoAAIAEAOBvB0B6uw2QbjtJYqllLLVkbD7bvh8yLw`
다. 기존 Rust `decode_ctk3_exact`로 복원한 문서는 폭 10, 높이 10, 단일 page이며,
operation·garbage·comment가 없다. 회색 고정 셀은 22칸, 색칠된 목표 셀은 56칸이다.

| 대상 | little-endian u64 words |
| --- | --- |
| 초기 점유 | `[0x000000c0383f3fc7, 0, 0, 0]` |
| 채울 영역 | `[0xfe3fff3fc7c0c038, 0x0000000c120fc1fc, 0, 0]` |

각 미노 색은 8칸씩이지만 색별 미노 identity 제약은 부여하지 않는다. 색칠된 셀의
합집합을 일반 Build의 required 영역으로 사용한다. 전체 56칸을 14번의 실제 lock으로
채우고, 원래 초기 점유·즉시 줄 삭제·최종 보드 조건을 보존한다. 그림을 정적 연결
타일링으로 한정하거나 과거 8행 예시로 축소하지 않는다.

공급은 전체 `P7P7`의 14 draw, 원본 큐 25,401,600개이며 oracle로 평가한다.
규칙은 SRS+, 초기 Hold는 비어 있고 Hold는 허용한다. `CountUnique`의 전체 공급
coverage를 평가하며 색별 identity filter·spin·score·finesse 조건은 넣지 않는다.
Auto worker 정책과 전체 논리 프로세서를 사용한다.

기존 compiler는 n14/N14의 빈 Hold 요청에 EOF held-release에 필요한
`projects_unplaced_lookahead` 권한을 발급한다. 이를 임의로 끄거나 원본 14 draw를
줄이지 않는다. 실제 compiler의 두 projection flag와 universe의 complete 여부를
실행 metadata로 확인한다.

## 2. 비용 비교와 완료 판정

기준 실행은 decode·problem compile·host 등록·실제 제품 실행을 구분하고 전체 비용도
기록한다. 상세 진단은 기존 opt-in stage profiler를 사용하며 생성·공급 인덱스·
도달성·coverage·메모리 집계 비용을 나눈다. 진단 timer의 샘플 가중 추정과 실제
전체 wall time을 같은 수치로 취급하지 않는다.

개선 채택에는 같은 입력·규칙·Hold·원래 ID 공간·worker 정책의 완료 결과를 사용한다.
중단 진단은 별도로 표시하고 부분 coverage나 중단 시간을 완료 확률·속도 개선으로
사용하지 않는다. 외부 manager의 메모리·시간 감독, 사용자가 명시한 예산, 표현의
실제 인덱스 한계, 취소·할당 실패 처리는 유지한다. 개발용 생성 횟수 상한을 제품의
기본 탐색 의미로 강제하지 않는다.

## 3. 작업 상태

- [x] 수정 파일: `crates/clearra-webgpu/`, `crates/clearra-postprocess-gpu/` 및 연결 소비자. 미사용 GPU 실행 구현·의존성·빌드 연결을 제거했고 기존 GPU 요청의 명시적인 퇴역 응답을 유지한다. `39833052` 및 후속 정리를 계승했고, `02b9c6af`에서 잔여 GPU 실행 표시도 정리했다.
- [x] 수정 파일: 제품 검색 기본값, `crates/clearra-core-executor/src/backend/wasm_cpu/`의 생성 owner와 budget 계약. `762ea94c`, `0d73a32e`, `130179c0`에서 implicit ordered-kick compiler 상한을 제거하고 Build·exact-cover 기본 생성 한도를 해제했으며 관측 실험 예산을 제품 경로에서 분리했다. 명시 예산과 overflow·취소·자원 실패 계약은 보존한다.
- [x] 수정 파일: `crates/clearra-app/examples/oracle_ctk3_bench.rs`. 주어진 CTK3와 전체 P7P7을 실제 native Build 제품 경로에서 실행하고 입력·완료 여부·원본 coverage·비용을 기록하는 하네스를 구현했다. 정식 v2의 실제 실행은 6절에 기록했으며, `02b9c6af`에서 worker 선택을 실제 Auto 정책에 맞췄다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/extended_buildup.rs`, `extended_reachability.rs`, `build_probability.rs`, `extended_build_probability.rs` 및 `solution_set_audit.rs`. 수직 진입 증명으로 reachability 비용을 줄이고 실제 live backing과 빈 audit 후보의 workspace 경계를 고쳤다. 동일 바이너리의 완료 A/B 결과는 10절에 기록한다.
- [x] 완료 목표: 동일 입력·규칙·전체 25,401,600개 원본 큐·Auto/9 worker 조건으로 P7P7 full-draw를 완주했다. 진단 옵션을 끈 기본 설정 실행은 탐색 27.707초, 하네스 전체 29.136초다.

지정 보드의 수 분 내 완료 목표는 달성했다. 기존 중단 진단은 역사 자료로 유지하며,
완주 속도의 비교에는 10절의 동일 바이너리 완료 실행만 사용한다. 이번 완료는
FullQueueOracle Build의 count·probability 범위이며, 11미노 see-7이나 별도
minimum-cover 증명의 완료를 뜻하지 않는다.

## 4. 비교할 축약의 정확성 의무

배치 후보 `C`의 실제 Build order graph가 허용하는 미노 종류 순서의 집합을
`L(C)`라 하고, 원본 큐 `q`와 고정된 Hold/EOF 규칙으로 실행할 수 있는 순서를
`A(q)`라 한다. Boolean buildability에서 원본 큐의 성공 조건은 정확히
`L(C) ∩ A(q) ≠ ∅`이다. 이동·킥·즉시 줄 삭제·최종 보드 조건은 `L(C)`를 만들 때
기존 물리 그래프로 판정한다. 이 물리 판정을 생략하는 정적 타일링 완화는 허용하지
않는다.

따라서 같은 종류 라벨과 같은 suffix 언어를 가진 그래프 노드를 정확하게 intern하고,
같은 라벨·같은 child의 중복 edge를 Boolean union으로 합치는 것은 `L(C)`를
보존한다. 완전한 standard bag의 기존 symbolic supply product가 같은 `A(q)`를
사용한다면, 각 원본 ID의 성공 여부와 후보 간 coverage union도 보존된다.
해시만 같은 노드를 합치지 않으며 정확한 edge tuple 비교를 유지해야 한다.

후보별 물리 경로 수, finesse, spin, score, 실행 제약, 제한 관측 정책은 Boolean
언어만으로 대체할 수 없는 별도 계약이다. 최초 전환은 해당 요구가 없는 일반
Build·완전한 Standard7Bag·FullQueueOracle에 한정하고, 나머지 경로는 기존
평가기와 인덱스를 유지한다. 대표 해답은 symbolic product가 찾은 원본 큐와 기존
물리 그래프를 다시 연결해 생성한다. 최소 원본 ID와 후보 순위 규칙도 유지한다.

symbolic cache를 비우기 전에 그 epoch의 coverage를 원본 ID 비트셋에 반영하고,
그 뒤에만 언어·decision ID를 폐기한다. 이 cache 용량은 재계산과 정확한 flush를
수반하는 저장 정책이며 탐색을 잘라내는 생성 한도가 아니다. 최종 bitmap은 모든
worker와 후보의 합집합을 원래 25,401,600개 ID 공간에서 산출해야 한다.

## 5. 모든 경로의 동치 근거

이 절의 명제는 아래 적용 조건을 만족하는 요청에서 **기존 물리 탐색의 모든 완성
경로를 고려한 각 원본 큐의 Boolean 성공 여부**를 보존한다는 것이다. 특정 보드나
샘플 큐의 테스트 통과를 전칭 증명으로 사용하지 않는다. 현재 물리 엔진의 표현·규칙
지원 범위를 새로 넓힌다는 명제도 아니다. 정상 완료한 결과의 의미와 자원 부족으로
완료하지 못한 실행의 상태는 별도로 다룬다.

아래 executor 파일명은 `crates/clearra-core-executor/src/backend/wasm_cpu/`를
기준으로 한다. 공급의 원본 축·multiset 구현은
`crates/clearra-supply/src/pattern_universe/`, 문제 compiler는
`crates/clearra-problem/src/search_problem.rs`, 실행 admission은
`crates/clearra-core-executor/src/resource/execution_admission.rs`를 참조한다.

### 5.1. 적용 조건과 증명 대상

`extended_build_probability.rs`의
`ExtendedBuildProbabilitySession::uses_standard_bag_coverage`는 다음 조건을 함께
검사한다. `new_standard_bag_coverage`는 여기에 세션 목적이
`ExtendedFamilyPurpose::Build`라는 조건을 추가한다.

이 입력의 기존 typed Build source authority는
`ProblemCompiler::compile_scenario_pc`가 만든 `ScenarioPc`/`Trace` 문제와 별도의
일반 Build field다. source preset 이름이 PC라는 이유로 완료 목표를 PC로 바꾸지
않는다. Build 세션의 목적과 required 영역이 완료 조건을 정한다.
`checked_build_probability_pointee_retained_bytes` 등 기존 메모리 계약의 허용 범위도
그대로 유지한다. `CoverageSummary` override를 붙여 이 authority를 대체하거나,
새 공급 evaluator를 선택했다는 이유로 getter/admission의 지원 범위를 넓히지 않는다.

- 집계는 `BuildProbabilityAggregation::Buildability`, count는 `CountUnique`,
  관측 정책은 `FullQueueOracle`이다.
- finesse, score, 실행 제약, solution probability 행을 요청하지 않고,
  `PcChanceEvidencePolicy`는 `Disabled`이다. 따라서 이 경로가 전체 물리 경로 수,
  finesse 비용, spin/score 증거, minimum-cover 등 별도 증거를 생성한다고 주장하지
  않는다. 기존 대표 물리 경로 하나는 아래 5.5의 방법으로 보존한다.
- 공급은 실제 저장 형식까지 인증된 완전한 uniform `Standard7Bag`이다. 원본 큐가
  잘리지 않았고, 실제 길이·개수·선언된 전체 개수와 fresh-bag cardinality가 일치한다.
- `StandardBagCoverage::supports`가 확인하는 초기 공급 상태는 cursor 0,
  fresh 7-bag remainder, bag epoch 0이다. 초기 Hold는 비어 있거나 정확한 한 종류다.
  Hold 금지와 초기 occupied Hold의 모순은 기존 compiler/validator 계약으로 배제한다.
- projected terminal window이면 standard-bag lookahead 권한이 함께 있고,
  원본 draw 수에 초기 held piece 수를 더한 값이 정확한 배치 수와 같다.
- 실제 마지막 원본 ordinal은 기존 `u32` witness 표현 범위 안에 있다.

나머지 요청은 기존 position index와 평가기로 간다. PC 목적의 verifier는 이 Build
경로를 발급받지 않는다. `apply_build_order_result`도 symbolic owner가 있을 때
finesse language, spin graph, PC observation owner, solution coverage 행이 섞이지
않았는지 재확인한다. 따라서 입구의 적용 조건과 실제 소비자의 권한이 일치한다.

### 5.2. 실제 원본 공급과 ordinal의 보존

원본 길이를 \(N\)이라 하고, fresh standard bag을 순서대로 뽑아 얻는 원본 집합을
\(\Omega_N\)이라 하자. 그 크기는 checked arithmetic으로 계산한

\[
P_N=\prod_{i=0}^{N-1}(7-(i\bmod 7))
\]

이다. `MaterializedPatternUniverse::uniform_compact_source`는 descriptor 문자열이나
일부 표본이 아니라 실제 `PatternSequenceStorage::Standard7BagLexicographic` 저장과
uniform weight를 검사한다. 새 gate는 그 결과의 길이·개수를 요청 및
`total_possible_pattern_count()`와 대조하고 `complete`와 truncation 부재를 확인한다.
명칭이나 cardinality가 우연히 같은 explicit/factorized/observed 공급은 이 권한을
얻지 않는다.

`standard_7_bag_sequence_space.rs`의 원본 ordinal과
`standard_bag_coverage.rs`의 draw별 decision branch는 같은 종류 순서와 bag remainder를
사용한다. 각 첫 draw의 ordinal 구간은 그 draw 뒤 가능한 원본 suffix 수만큼의
연속 구간이다. 깊이 0에서는 같은 전체 구간이고, 같은 draw를 고르면 두 표현 모두
같은 구간으로 좁혀지므로 깊이에 대한 귀납으로 모든 완전한 큐 \(q_i\)의 ordinal
\(i\)가 같다. 뒤에서 설명하는 결정된 lookahead는 이 원본 구간에 새 축을 추가하지
않는다. 원본 source identity와 weight owner도 교체하지 않는다.

`piece_multiset_group.rs::symbolic_standard_bag_family_bounded`는 각 geometry multiset
group에 원본 전체 pattern set을 붙인다. 따라서 해당 경로의
`ExtendedGeometrySearch::new_bounded_with_pattern_indexes`가 position index 생성을
생략해도 별도의 candidate별 원본 ID allowlist를 지우지 않는다. multiset family가
유한 공급 창의 실제 성공보다 넓은 후보를 포함하는 경우에도, 마지막 exact 공급
product가 그 후보의 성공 큐를 다시 판정한다. 기하 후보의 생성 순서·row ID·초기
보드·required 영역은 그대로다.

### 5.3. 물리 그래프에서 종류 언어로의 귀납

고정 후보 \(C\)의 기존 exact Build graph를 \(G_C\)라 하자. 노드 \(v\)에서
accepting terminal까지 가는 물리 경로의 종류 투영을 \(L_G(v)\)라고 정의한다.
accepting 노드는 outgoing edge가 없고, 모든 edge는 lock 깊이를 하나 증가시킨다.
이는 `buildup.rs::BuildOrderGraph::from_topological_parts`가 검사한다.

\[
L_G(v)=
\begin{cases}
\{\epsilon\}, & v\text{가 accepting},\\
\displaystyle\bigcup_{(v\xrightarrow{p}u)\in E}pL_G(u), & \text{그 외}.
\end{cases}
\]

accepting terminal에 도달할 수 없는 노드는 빈 언어다. 물리 엔진은 현재 보드에서의
진입·이동·회전/킥·lock·즉시 줄 삭제와 후보의 최종 조건을 종전과 같은 방법으로
판정한다. `extended_buildup.rs`의 graph 생성 결과를 그대로 받으며, 새 공급 경로는
그 reachability를 정적 타일링 조건으로 대체하지 않는다.

`piece_order_language.rs::PieceOrderLanguageCache::canonicalize`는 역위상 순서로
위 식을 계산한다. terminal의 언어는 동일하다. 모든 child가 자신의 원래 언어를
보존한다고 가정하면 부모는 각 `(piece, child)`의 prefix-union을 취하므로 같은
언어를 갖는다. 동일 `(piece, child)`의 중복은 집합 union의 멱등성으로 제거할 수
있다. 같은 종류지만 서로 다른 child인 NFA edge는 모두 남으므로 한 종류의
대안을 임의로 하나만 고르지 않는다.

interning은 accepting/depth/edge tuple의 정확한 일치를 확인한다. 해시 충돌은
동일성 증거가 아니다. 이 귀납은 \(L_G(v)=L_K(k(v))\)를 모든 live 노드에 대해
보이며, structural interning이 모든 언어 동치까지 찾아내는 최소 DFA라고 주장할
필요는 없다. 찾은 동치만 정확하면 충분하다.

종류만 같은 서로 다른 물리 경로가 한 단어로 합쳐져도, 이번 관측량은 그 단어의
**존재 여부**다. 경로 수·경로별 비용·회전 상태별 증거가 필요한 요청을 제외한
이유가 여기에 있다. 원래 graph의 operation edge와 pose는 삭제하지 않는다.

### 5.4. 같은 Hold automaton과 terminal projection

원본 큐 \(q\)에 대해 기존 공급 규칙이 허용하는 배치 단어 집합을 \(A(q)\)라 하자.
공급 product 상태는 종류 언어 노드, 소비한 원본 draw 위치, 정확한 bag remainder,
Hold의 유무와 종류로 결정된다. `standard_bag_coverage.rs`의
`StandardBagCursorTransitions::compile`은 기존
`SupplyExecutionAutomaton::advance_bag_cursor`에서 cursor 전이를 만들고,
`matching_supply_step`과 `solve`는 Current, SwapHeld, StoreCurrent의 같은 소비
조건을 적용한다. empty-Hold Store는 현재 draw와 다음 draw가 둘 다 허용될 때만
한 lock을 만든다.

남은 lock 수에 대한 귀납을 사용한다. terminal에서 아래 종료 조건이 같고,
각 비terminal에서 동일한 공급 행동과 동일한 종류의 모든 child를 방문한다.
귀납 가정으로 child의 성공 원본 집합이 같으므로 그 합집합도 같다. occupied Hold와
current가 같은 종류인 경우 Current와 SwapHeld는 다음 cursor·bag·held가 모두
같다. Boolean union에서는 한 번만 계산할 수 있지만 CountAll 이력 수에는 이
축약을 적용하지 않는다.

종료 조건은 별도로 중요하다. compiler의
`search_problem.rs::piece_source_materializer::resolve_supply_window`에서 projected
terminal window를 발급하면, 배치 수 \(n\), 원본 draw 수 \(N\), 초기 held piece 수
\(I\in\{0,1\}\)는 \(n=N+I\)를 만족한다. 초기 Hold가 비어 있으면 최초 Store만
추가 draw 하나를 소비하고 그 뒤 Hold는 occupied로 유지된다. 초기 occupied Hold는
그 최초 Store가 없다. 따라서 정상 완료 상태는 기존
`buildup.rs::accepts_pattern_path`와 같은 다음 경우다.

- 원본 draw를 모두 소비하고 Hold가 비어 있다.
- 마지막 projected 소비까지 진행하여 cursor가 \(N+1\)이고 Hold가 occupied다.
- 구체적 다음 종류가 없는 EOF에서는 accepting child로 가는 held piece의 마지막
  해제를 기존 `can_release_held_at_terminal` 규칙으로 처리한다.

이 관계를 임의의 \((n,N,\text{projection})\) 조합으로 일반화하지 않는다.
projection이 없는 경우에는 기존과 같이 남은 공급 tail을 모두 소비해야 한다는
추가 조건을 만들지 않는다.

\(N\bmod7=6\)이면 마지막 bag의 남은 종류가 하나라 다음 종류가 원본 큐로부터
확정된다. 이때 `solve`는 그 종류를 Current로 쓰거나 held와 Swap하는 기존 전이를
허용한다. 마지막 원본 draw에서 empty-Hold Store를 하는 경우의 확정된 next도 같은
규칙을 따른다. 결정된 종류는 새 원본 ordinal을 만들지 않는다. 해당 위치에서
held-only EOF 해제를 별도로 적용해 current에 대한 기존 규칙을 우회하지 않는다.
대표 경로의 해당 Current 결정은 기존 `use-unplaced-lookahead` label로 보존한다.
`coverage_product.rs`의 projected position 조회와
`terminal_hold_projection.rs::terminal_release_pattern_word`도 같은 경계를 쓴다.

실제 N14/n14 요청은 완전한 두 bag 뒤에 미지의 새 bag이 시작한다. 다음 종류를
가정하여 배치하지 않으며, 기존 권한에 따른 마지막 held piece의 해제만 허용한다.
초기 Hold가 있는 일반화에서는 \(n=N+1\) 관계를 유지하고 그 held 종류도 공급
상태에 포함한다. 이로써 원본 14 draw를 줄이거나 임의의 15번째 종류를 성공 확률의
새 표본으로 넣지 않는다.

### 5.5. 후보별 성공·최소 witness·전역 합집합

각 후보의 성공 ordinal 집합을

\[
S_C=\{i\in[0,P_N):L_G(\operatorname{root}_C)\cap A(q_i)\ne\varnothing\}
\]

라고 하자. 5.3과 5.4로 canonical product도 정확히 \(S_C\)를 계산한다. 기존 Dense
평가기는 이 집합을 원본 bitset에 표시하고, StandardBagCoverage는 같은 집합을 DD로
표시한다. DD의 summary와 materialization은 같은 원본 ordinal 구간을 사용하므로
cardinality와 최소 원본 ID도 같다.

완전한 원본 축이라는 조건 때문에 `covers_any_pattern`은
\(S_C\ne\varnothing\)와 같다. truncated 축에서는 full-bag DD의 성공이 보관된 원본
prefix 바깥에만 있을 수 있으므로 이 결론을 사용할 수 없다. 따라서 새 gate는
truncation을 단순히 count를 잘라 처리하는 대상으로 보지 않고 기존 경로로 돌린다.

`ExtendedBuildProbabilitySession::apply_symbolic_build_order`는 그 최소 원본 큐를
원래 `BuildOrderGraph`와 `representative_pattern_path`에 다시 넘긴다. 같은 큐,
같은 operation edge 순서, 같은 Hold 규칙으로 기존 witness 탐색을 실행하므로
canonical edge의 저장 순서가 대표 물리 경로를 바꾸지 않는다. 후보 선택은 기존
external ordinal 또는 geometry candidate ordinal의 최소 순위 규칙을 유지한다.

성공 후보 identity의 집합, 후보별 covered count, 기존 정의에 따른 그 count의 합도
같다. 전체 coverage는 \(S=\bigcup_C S_C\)이며 union의 교환·결합·멱등성 때문에
후보 처리 순서나 worker 분할에 영향을 받지 않는다. 이 명제는 원본 ID별로
성립하므로 같은 weight owner에서 계산한 확률도 같다. coverage가 원본 전체를
채웠다는 이유로 남은 후보 탐색을 중단하지 않는다. 그렇게 중단하면 별도의 후보
수·solution identity·대표 후보 순위 계약까지 보존한다는 결론을 낼 수 없다.

### 5.6. worker 권한과 cache epoch

native 공유 준비는 `ExtendedBuildSharedPreparation`이 같은 원본
`Arc<SearchProblem>`, catalog, target 목록, field, aggregation을 붙잡는다.
`shared_build_preparation`에서 발급한 capability를
`new_shared_build_verifier`가 소비하며, 별도의 SearchProblem이나 외부 catalog ID로
바꾸지 않는다. pass의 field/aggregation도 대조한다. 기존 finite node-budget 위임,
mirror, finesse 등 이 공유 준비가 지원하지 않는 경우는 기존 constructor를 쓴다.

공유되는 것은 immutable 입력이다. 각 worker의 reachability workspace, kind arena,
StandardBag DD/memo, reducer와 대표 경로는 새 private owner다. 숫자 kind/DD root는
다른 worker로 보내지 않는다. `complete_distributed_worker`와 정상 완료 모두
`build_result`를 거치며, 시작 시 `finalize_standard_bag_coverage`가 원본 bitmap을
확정한다. coordinator는 기존 `validate_distributed_solution_surface`와
`absorb_distributed_result_with_memory_guard`의 원본 축·완전성·identity 검사를
유지하고 원본 bitmap을 OR한다. 부분 worker를 전체 완료로 승격하지 않는다.

cache 재활용은 다음 불변식으로 설명할 수 있다. 이미 materialize한 원본 집합을
\(B\), 현재 DD epoch의 집합을 \(D\)라고 하면, 처리한 후보의 결과는 항상
\(B\cup\operatorname{decode}(D)\)다. 새 후보의 DD를 \(D\)에 union하면 위 식이
그 후보의 성공 집합만큼 늘어난다. `flush_and_recycle_local_cache`가
\(B\leftarrow B\cup\operatorname{decode}(D)\)를 먼저 수행하면, 그 뒤 DD와 memo를
비워도 식은 같다.

그 다음에만 `PieceOrderLanguageCache::clear_retain_capacity`로 kind ID를 폐기한다.
StandardBag product memo가 숫자 kind ID를 key에 사용하므로 이 순서를 바꿀 수 없다.
새 epoch에서 같은 숫자 ID가 다른 언어를 뜻해도 이전 memo가 남아 있지 않아 혼동이
없다. candidate의 summary/witness와 필요한 global merge는 해당 epoch가 살아 있을
때 소비한다. cache 용량 기준은 이 flush와 재계산을 유발하는 저장 정책이며,
기하 후보·공급 draw·가능 경로를 잘라내는 생성 상한이 아니다.

### 5.7. 자원 admission과 불완전성의 분리

종류 언어에서 새로 필요한 node 수는 기존 live graph node 수 이하이고, 새 edge 수는
live piece edge 수 이하이다. interning은 이 개수를 늘리지 않는다.
`PieceOrderLanguageCache::checked_canonicalize_future_bytes`는 이 상계와 scratch,
bucket, 기존·신규 buffer의 공존을 사용하고, caller는 canonicalization 전에 같은
실행 memory bound로 admission한다.

StandardBag DD의 union 중간 크기를 단순히 physical node 수에 비례한다고 가정하지
않는다. `StandardBagCoverage::checked_creation_future_bytes`로 request table을
사전 admission하고, `prepare_standard_bag_resources` /
`StandardBagCoverage::set_resource_boundary`로 DD 외부의 현재 retained bytes를
결합한다. DD 재귀 중에는 그 외부 owner가 변하지 않는다. 필수 node 저장은
`reserve_node_storage`가 여러 buffer의 replacement 전체를 첫 할당 전에 검사한다.
원본 global words와 결과 bitmap도 같은 bound에 포함한다.

optional memo·interning 저장이 거절되면 기존 exact 계산을 재수행하거나 cache 삽입을
생략할 수 있다. 이는 논리적 REJECT와 다르다. 필수 저장·표현 index·checked 산술이
실패하거나 취소되면 해당 오류/불완전성 계약을 유지하며, 계산하지 못한 후보를
불가능하다고 확정하지 않는다. 기본 생성 횟수 상한을 제거하는 작업 역시 명시적인
사용자 예산, 실제 표현범위, 이 allocation admission과 외부 manager 감독을 없애는
일이 아니다.

`ExecutionAdmissionPlan::symbolic_build_probability_with_verifiers`가 생략하는 것은
새 경로에서 생성하지 않는 position index와 explicit queue 배열의 비용이다. 원본
ID bitmap, worker별 mutable owner, finalization의 동시 보유와 기존·신규 재할당
peak는 계속 계상한다. 작은 준비 projection이 통과했다는 사실만으로 후속 DD
전체가 예산에 들어간다고 결론 내리지 않는다.

따라서 위의 언어 동치는 정상 완료한 계산의 결과를 보존하는 명제다. 기하 생성이나
product가 명시 예산·할당 실패·취소로 끝나지 못한 실행은 그 상태대로 보고해야 한다.
부분 coverage와 incomplete count를 전체 확률, 정확한 0%, 또는 완주 성능으로
표현하는 것은 이 증명에서 허용되지 않는다.

### 5.8. 회귀의 역할과 명제의 범위

`extended_standard_bag_tests.rs`는 N2 EOF 해제, N6의 결정된 마지막 종류, 초기
occupied Hold, 같은 종류의 서로 다른 NFA suffix, 중복 물리 이력, DD flush 뒤의
numeric kind ID 재사용, truncated/factorized/PC evidence gate를 대조하는 작은
회귀를 둔다. 실제 8행 I+O Build 입력에서는 기존 physical producer를 양쪽에서
실행하고 Dense와 symbolic 결과의 원본 ID·identity·후보 순위·대표 경로를 비교한다.
그 fixture도 위 `ScenarioPc`/`Trace` source authority를 사용한다.
이 회귀는 구현의 경계 위반을 찾는 보조 증거이며, 유한 사례를 전체 입력에 대한
증명으로 대체하지 않는다. 실행 결과와 측정 상태는 이 절 밖의 검증 기록으로 관리한다.

전칭 결론의 근거는 5.2의 원본 축 귀납, 5.3의 graph 언어 귀납, 5.4의 공급 product
귀납, 5.5의 원본 ID별 집합 등식, 5.6의 owner/epoch 불변식이다. 그 조건 아래서는
미노 수나 실제 branch 선택에 따라 예외 경로를 두지 않는다. CountAll, 비완전 공급,
제한 관측, 경로별 증거 및 기존 물리 엔진 밖의 규칙·표현범위에는 이 Boolean
치환의 결론을 확장하지 않는다.

## 6. 최초 제품 실행과 측정 경계

첫 하네스 v1은 일반 Build source에 `CoverageSummary`를 덧씌웠다. 이는 기존
`ScenarioPc`/`Trace` authority의 지원 범위를 벗어난 입력 어댑터 오류다. 해당 실행은
진단 이력으로만 보존하고 성능 비교 기준에서 제외했다. 제품의 resource getter를
넓히지 않고 하네스를 기존 기본 `Trace` 계약으로 고쳤다.

정식 v2 입력은 source commit `0255dd6016812302169a19efecd0111e5a1ba00d`로
빌드한 동일 실행 파일에서 검사했다. SHA-256은
`cb9916dd990f1454b424b887db3f8dd2759920661f644ce1c56a4fdf689dfaf3`이다.
`parallel,local-search-ab,wasm-stage-profiling` 진단 빌드이며 실제 native Build
host를 거친다. 원본 25,401,600개, 14 draw, 두 projection flag, `Trace`,
Auto/9 worker, 전체 원본 공급의 완전성을 확인했다.

| 실행 | 상태 | 관찰 |
| --- | --- | --- |
| `CLEARRA_EXTENDED_STANDARD_BAG_COVERAGE=0` | 탐색 시작 전 admission 거절 | 기존 인덱스 준비 계획이 3,703,235,872 bytes를 요구했다. 1 GiB aggregate 계약을 올리지 않았다. |
| symbolic 기본값, 120초 진단 | 120.0254초에서 요청 취소, incomplete | catalog 0.00058초, supply 0.0161초, geometry span 약 1.529초, caller memory accounting 약 0.199초. sampled process-tree RSS peak 181,706,752 bytes. |

첫 진단에서는 native Build worker에 profiler TLS token이 전달되지 않아
`pooled_workers`가 비어 있었다. 따라서 위 caller 시간만으로 physical graph와
symbolic supply product 중 어느 쪽이 병목인지 판정하지 않는다. native worker의
요청 수명에 기존 `WorkerSearchProfile`을 연결하고, 모든 thread join 뒤에 caller가
profile을 마감하도록 보완했다. 일반 제품 빌드에서는 기존처럼 no-op이다.

기존 native 진행 이벤트의 `completed=32`도 누적 후보 수가 아니라 배치 크기였다.
이 값은 전체 처리가 32개에서 멈췄다는 증거가 아니다. 동일 배치 전송 경계에서
누적 dispatched 후보 수를 보고하도록 고쳤다. 이 진단 실행을 완주 결과나 속도
개선 비율로 사용하지 않는다.

원시 자료는 `_local/artifacts/oracle-p7p7-20261122/`의
`baseline-v2-dense.json`, `optimized1-profile120.json`과 대응 stderr에 보존했다.
외부 감독 receipt는 각각 `1795352229764967539-6-runtime.json`,
`1795352234083267055-6-runtime.json`이다. 후자는 감독 강제 종료나 메모리 압력이
아니라 하네스가 요청한 취소를 정상 전파한 nonzero 종료다.

## 7. 작업 공간 복구와 계승

2026-11-22에 `/workspace/scratch/c01fc07d23b8/` 아래 Clearra 작업 공간 32개와
관련 branch의 패치를 조사했다. 후속 통합 및 동일 패치를 확인한 뒤, 깨끗한 최신
통합 원본인 `/workspace/scratch/c01fc07d23b8/clearra-oracle-p7p7`의 `bedd04ae`를
계승 기준으로 선택했다. 별도 작업 공간의 미커밋 높이 5·6 검증 테스트는 원위치에
보존했다. 이전 branch의 변경을 중복 적용하지 않는다.

계승한 과거 실행·검증 자료의 기준 경로는
`/workspace/scratch/c01fc07d23b8/clearra-oracle-p7p7/_local/artifacts/oracle-p7p7-20261122/`
다. `core-full-tests.log`, `validation-tests.log`, `full-draw-prune-tests.log`는
이전 검증 기록이며 새 작업 공간에서 재실행한 결과로 표시하지 않는다.
`baseline-v2-dense.json`은 admission 거절이고 `optimized1-profile120.json`은
120초 요청 취소에 따른 incomplete 결과로, 어느 쪽도 P7P7 완주 기록이 아니다.
마지막 `optimized2-build.log`는 `clearra-app` 컴파일 단계에서 기록이 끝나며,
갱신된 하네스의 빌드 완료나 후속 실측을 확인할 자료는 없다.

현재 작업 공간은 `/workspace/scratch/333e8657ccd3/Clearra-resumed`이며 branch는
`codex/oracle-p7p7-resumed-20261122`다. 계승 뒤 `02b9c6af`에서 퇴역 GPU 실행
표시의 잔여 연결을 제거하고 하네스의 worker 선택을 실제 Auto 정책으로 수정했다.
계승 뒤의 구현·검증·실측은 아래 절에 별도로 기록한다.

## 8. 수직 진입 경로 증명과 회귀 검증

`8ac953f0`의 `ExtendedHarddropReachability`는 각 물리 보드에서 점유 셀의 아래쪽
column shadow를 한 번 계산한다. 유효한 catalog lock mask가 그 shadow와 겹치지
않으면, 같은 x·rotation으로 보드 위에서 해당 위치까지 내려오는 모든 중간 pose가
비어 있다. 기존 extended BFS는 field height 이상의 모든 유효 pose를 seed로 두고
아래 이동을 포함하므로, 바닥 또는 바로 아래 점유 셀에 의해 grounded인 mask에는
실제 translation-only 경로가 존재한다. 4개 u64 word를 넘는 shift carry도 유지한다.

이 검사는 reachability의 충분조건이다. 증명 실패를 unreachable로 해석하지 않고
기존 전체 BFS를 실행한다. 회전·kick 증거를 요구하는 spin/scoring, B2B, finesse는
shortcut 권한에서 제외한다. catalog realization 순서, 기존의 negative membership,
child 발급 순서와 첫 accepted edge를 보존하므로 원래 physical graph와 대표 경로를
그대로 유지한다. 별도의 큰 cache나 후보·node 생성 한도는 추가하지 않았다.

native `local-search-ab` 빌드의 `CLEARRA_EXTENDED_HARDDROP_PROOF=0|1`로 비교할
수 있다. 제품 기본값은 활성화이며, `CLEARRA_EXTENDED_REACHABILITY_PREFILTER=0`은
기존 eager BFS를 먼저 수행하므로 새 shortcut의 성능 비교에는 사용하지 않는다.

| 새 검증 | 결과 |
| --- | --- |
| 완전한 graph/edge, CountAll, 원본 큐별 coverage·대표 witness 대조 | 통과. I의 줄 삭제와 shelf 아래 O의 side-entry를 섞어 proof/fallback 및 삭제 frame을 함께 검사했다. |
| 독립적인 cell sweep 및 전체 BFS 대조 | 통과. SRS90/SRS+, 높이 24까지, 네 Board256 word 모두에서 positive proof를 검사했다. |
| 기존 physical/finesse/scoring/B2B 및 순환 inverse-dependency 회귀 | 통과. |
| 전체 core-executor lib 회귀 | 1,250 passed, 0 failed, 14 ignored; 157.13초. |

전체 회귀의 첫 기본 병렬 test-harness 실행은 92건의 shared-resource admission
contention으로 실패했다. 같은 설정에서 Rust test-harness만 `--test-threads=1`로
직렬화한 위 실행이 통과했다. 제품 solver의 Auto worker 설정은 변경하지 않았다.
자료는 `_local/artifacts/harddrop-proof-20261122/`에 보존한다.

## 9. 복구 뒤 첫 실측과 메모리 진단

아래 두 실행은 원본 입력, Auto/9 worker, 기존 1 GiB native aggregate 예산을
유지했다. 두 실행 모두 `shared_execution_memory_exhausted`로 끝났으므로 **완주
확률이나 완주 속도의 개선율을 제시하는 근거로 사용하지 않는다.** 첫 실행은
계승 source ID `bedd04ae`와 GPU 표시 정리/하네스 작업이 겹친 진단 빌드다.
정식 채택 비교는 이후 같은 완성 바이너리의 A/B 결과로 수행한다.

| 실행 | 실제 execution 시간 | sampled process-tree RSS peak | 상태 |
| --- | ---: | ---: | --- |
| 하드드롭 개선 전 진단 | 38.6757초 | 186,593,280 B | 내부 memory admission 거절, incomplete |
| `8ac953f0`, harddrop=1 | 28.5362초 | 190,889,984 B | 같은 오류, incomplete |

worker의 `wasm.candidate.build_order_reachability` span은 각각 약 66.715초,
53.225초이고, `coverage_language_product`는 약 1.143초, 0.743초였다. 이들은
병렬 worker의 중첩 span 합계이며 일부 호출의 샘플 가중 추정도 포함한다. 실제
wall time이나 exclusive CPU time으로 더하거나 나누지 않는다. BuildOrder span에는
BFS 외에도 projection·줄 삭제·graph 생성 비용이 포함된다.

외부 supervisor의 physical-pressure 이벤트는 두 실행 모두 없었다. native Build는
전체 예산을 producer/verifier의 고정 몫으로 나누므로, RSS만 보고 budget 판정을
틀렸다고 단정하지 않는다. `20c9365e`에서 native `local-search-ab` 전용의
`CLEARRA_TRACE_MEMORY_ADMISSION=1` 진단을 추가했다. 실패 시 cap, 현재 계수된
backing, 미래 allocation, 요구 합계, caller를 기록하며 성공 경로와 admission
판정은 바꾸지 않는다. 같은 하네스에 정규 해답 key의 길이-prefix SHA-256과 대표
경로도 추가해 이후 완주 A/B에서 원본 bitmap뿐 아니라 해답 identity를 비교한다.

원시 자료는 `_local/artifacts/oracle-p7p7-resumed-20261122/`의
`baseline-profile120.json`, `harddrop-profile300.json`, 대응 stderr와 supervisor
receipt에 보존한다. 두 파일명의 시간은 진단 deadline이며 실제 종료 시간이 아니다.

### 9.1. 최종 결과의 historical peak 재계상

`20c9365e`의 진단 실행은 28.8403초에 같은 오류를 재현했다. 처음의 dense geometry
memo 거절은 기존의 정상적인 optional-cache fallback이며 terminal 오류와 구분했다.
마지막 거절은 main thread의
`extended_build_probability.rs::ensure_materialized_result_bound`였다.

| 항목 | bytes |
| --- | ---: |
| producer replica cap | 117,434,794 |
| session의 계수된 현재 backing | 3,990,374 |
| 결과에 대해 요구한 추가량 | 135,297,414 |
| 거절한 합계 | 139,287,788 |

추가량은 실제 결과 payload를 나타내지 않았다.
`build_probability.rs::checked_public_result_bytes`가 실제
`CoreExecutionResult::checked_resource_retained_bytes()`와 보고 필드
`resource_peak_cpu_bytes` 중 큰 값을 반환했다. 이 결과의 보고 peak에는 이미 종료한
verifier들의 과거 scratch peak 합이 포함되어 있었다. 따라서 최종 결과의 현재
메모리 경계에서 과거 worker 비용을 다시 예약하고 producer의 작은 replica cap과
비교했다. 현재 session backing은 약 4 MB이므로 historical Build scratch의 일반적
계수 문제를 이번 terminal 실패의 원인으로 대신 지목하지 않는다.

수정은 보고 peak와 현재 소유 메모리의 역할을 구분한다. live-result guard에는 실제
retained backing을 사용하고, 과거 peak는 결과의 resource report에 그대로 둔다.
전체 1 GiB, producer/verifier 분할, Auto worker, 물리 탐색·공급·coverage 의미는
변경하지 않는다. 최종 채택 여부는 동일 예산의 전체 목표 실행과 경계 회귀로
확인한다.

### 9.2. 빈 coverage 후보에 대한 audit workspace 예약

`de0c66b8`은 위 finalizer 경계를 통과했다. 이어지는 실제 제품의 solution-set audit에서
다른 memory projection이 드러났다. 같은 1 GiB aggregate 계약의 outer merger가
cap 1,056,913,152 B, retained 4,001,334 B, future 3,365,427,141 B를 보고했다.
이는 앞서 고친 historical peak와 별개의 예약이었다.

이번 요청은 해답별 확률/coverage 행을 요청하지 않는다. normalized solution key와
전체 원본 bitmap은 있지만, audit의 solution-coverage candidate는 0개다.
`SolutionSetAuditReport::checked_analysis_memory_projection`은 이런 경우에도
`candidate_count × 8 + 16`개의 full-universe pattern-ID 배열을 가정했다.
실제 `analyze_inner`의 class/family loop는 한 번도 실행되지 않으므로 이 분기에서는
coverage matrix, union 및 exact portfolio workspace가 생성되지 않는다.

수정은 `candidates.is_empty()`일 때만 이 pattern 항을 required bitmap의 snapshot
hash에 필요한 lazy dense-cache 변환 peak로 바꾼다. `PatternBitSet`의 기존
`checked_all_projection(...).constructor_peak_bytes`를 사용하므로 sparse 입력에서
Vec와 Arc word payload가 공존하는 경우도 계수한다. 이 입력에서는 bitmap 하나가
3,175,200 B이며 전환의 두 payload는 6,350,400 B다. 후보가 하나라도 있는 경로의
기존 projection은 유지한다.

normalized key 정렬·중복 제거·hash, global bitmap의 set bits, snapshot identity,
7개 audit stage, missing-coverage/no-family 상태 및 최종 actual-retained guard는
그대로 실행한다. 해답별 coverage가 없다는 감사 상태를 complete로 바꾸는 수정이
아니다. 원시 재현 자료는 `live-result-fix-off-incomplete.json` 및
`final-boundary-diagnostic.json`과 대응 stderr/receipt에 보존한다.

## 10. 최종 P7P7 full-draw 완주와 동일 바이너리 비교

측정 source commit은 `91f558ed7ea1689ad62587aa08a5d7e5b5c7db15`이며 실행 파일의
SHA-256은 `571d3f0ecf6d8abcd4b70e5d44e3b6ecd06f6ddf14a03406f475b07c97fb604b`다.
아래 세 실행은 모두 이 바이너리를 사용했다. 후속 문서 커밋은 측정 source와
구분한다. 실행 파일은 `parallel,local-search-ab,wasm-stage-profiling` 기능을 포함한
release 빌드이며, 세 번째 실행은 이 진단 기능의 runtime 옵션을 모두 끈 기본
제품 경로다. 별도 공식 릴리스의 배포 검증을 주장하지 않는다.

### 10.1. 완료 결과와 비용

입력은 1절과 같은 높이 10의 일반 Build다. 원본 14 draw와 EOF held-release,
SRS+, 빈 초기 Hold, 색칠된 56칸의 합집합, Auto/9 worker, 기존 native aggregate
1 GiB 예산을 유지했다. `max_patterns=0`이고 원본 공급에 truncation이 없다.
9 worker는 producer 1개와 verifier 8개의 기존 정책이다.

| 실행 | execution wall time | 하네스 전체 wall time | reachability states | 관측 process-group RSS peak |
| --- | ---: | ---: | ---: | ---: |
| Harddrop OFF, profiler ON | 38.428430257초 | 39.268715469초 | 1,686,742,017 | 211,509,248 B |
| Harddrop ON, profiler ON | 28.552926125초 | 29.762976648초 | 1,108,789,753 | 190,414,848 B |
| 기본 설정, profiler OFF | 27.706999533초 | 29.136275210초 | 1,108,789,753 | 239,538,176 B |

동일 profiler 조건에서 harddrop proof를 켠 실행은 execution 시간이 9.875504132초,
**25.6984% 감소**했고 1.3459배의 속도를 보였다. 탐색한 reachability 상태는
577,952,264개, **34.2644% 감소**했다. 이는 완료 실행 한 쌍의 관측값이며 반복
측정의 평균·분산이나 다른 보드의 개선율로 일반화하지 않는다. 과거 120초 중단
기록과 이번 완주 시간으로 별도의 배수를 계산하지 않는다.

RSS는 Linux supervisor의 저빈도 process-group 표본이다. kernel 전체 peak나
내부 admission backing과 동일하지 않으며, 세 실행의 RSS 차이만으로 일반적인
메모리 절감률을 주장하지 않는다. 세 receipt는 모두 정상 종료, exit 0,
memory-pressure event 0, automatic retry false다. 외부 manager의 300/330초
감독 timeout과 내부 1 GiB 예산은 서로 다른 계약이다. 기본 설정 실행에는 하네스
취소 deadline도 없으며 모든 local override가 null이다.

| 완료 결과 | 세 실행 공통값 |
| --- | ---: |
| 원본 큐 / 성공한 원본 큐 | 25,401,600 / 25,401,600 |
| 실패한 원본 큐 | 0 |
| oracle coverage probability | 1.0 (100%) |
| 정규화된 고유 해답 key | 570 |
| raw geometry candidate / family | 210,452 / 210,452 |
| 실제 Build graph node | 1,239,703 |
| 대표 원본 pattern ID | 1,249,920 |
| 대표 후보 ordinal | 616 |
| `complete`, `count_complete`, `probability_complete` | 모두 true |
| cancellation / resource truncation | 모두 false |

oracle 100%는 전체 큐를 미리 아는 조건에서 각 원본 큐에 대해 성공하는 배치
경로가 존재한다는 뜻이다. 모든 큐에 같은 고정 경로를 적용할 수 있다는 뜻이나
see-7의 제한 관측 정책이 완성됐다는 뜻은 아니다. 해답별 coverage 행과 최소
portfolio를 이번 요청이 요구하지 않았으므로, 별도 solution-set audit의 해당
단계는 기존대로 incomplete다. 이 상태를 count·probability 완료로 덮지 않았다.

### 10.2. 결과 동치 확인

ON/OFF의 `summary_fields`는 각각 중복 key 없이 198개다. 이 중
`total_reachability_states`만 다르고 나머지 197개는 정확히 같다. 기본 설정 실행은
ON의 198개 배열과 값·순서까지 동일하다. 세 실행의 `build`, `fixture_input`,
`source`, `cpu`, `coverage_bitmap`, `solutions` 객체도 모두 같다. 별도 검토자가
같은 JSON들을 독립적으로 비교해 이 결과를 확인했다.

| identity | 공통값 |
| --- | --- |
| 원본 bitmap word 수 | 396,900 |
| 원본 bitmap SHA-256 | `f8efc3f4e1a2947be585e7d04f3d1d03b7f3ed7a7e66a061e58cc6fd2b1abcb4` |
| ordered normalized key payload SHA-256 | `ec9ed9757ab5fbf725dbbecb1ef2c54741dca1687551d5acf6b6fef3f3684930` |
| normalized solution set hash | `ctks1:15b1df978d112a82` |
| raw candidate digest | `4a3396cd978dc6e7` |
| source / universe / weight identity | `9283098173179132030` / `12306225105732353076` / `13871145873042404851` |

key SHA-256의 인코딩은 정렬된 UTF-8 key 각각 앞에 u64 little-endian byte length를
붙인 것이다. 원본 bitmap은 원래 pattern ID 순서의 little-endian u64 word다.
대표 경로는 미노 종류뿐 아니라 rotation, x, y, Hold 행동, 줄 삭제 수의 14단계
전체가 동일하다. 실제 배치 순서는 `S I Z J T L I O T S Z J L O`이고 마지막 O는
`release-held-at-terminal`이다. 이 순서는 원본 입력 큐를 뜻하지 않는다.

### 10.3. 병목 계측과 개선 범위

| profiler span | OFF 누적 span | ON 누적 span |
| --- | ---: | ---: |
| worker Build order / reachability | 71.537705335초 | 51.497036363초 |
| worker coverage language product | 1.144158351초 | 1.058629567초 |
| caller geometry advance | 1.680377581초 | 1.538988753초 |
| caller candidate memory accounting | 0.207278060초 | 0.205206875초 |

이들은 중첩·병렬 span 합계이며 일부 샘플 가중 추정을 포함한다. exclusive CPU
time 또는 wall time으로 합산하지 않는다. 측정에서 큰 비용은 물리 Build order와
reachability에 남아 있고, 8절의 수직 진입 증명이 이 부분의 전체 BFS 작업을
줄였다. worker graph의 node 수나 공급 product의 결과를 줄여 얻은 개선이 아니다.

계승한 full-draw multiset filter와 symbolic Standard7Bag coverage는 유지했다.
이번 추가 변경은 물리 경로의 충분조건 증명, 과거 peak를 현재 결과에 재예약하던
오류 수정, 실제로 생성하지 않는 audit workspace의 예약 수정이다. 9.1과 9.2의
두 메모리 수정으로 최종 결과를 기존 예산 안에서 반환할 수 있게 됐다. GPU나
worker 수·메모리 예산 증가는 이번 완주의 조건이 아니다.

### 10.4. 최종 검증 범위

| 검증 | 확인한 결과 |
| --- | --- |
| `de0c66b8` 전체 core-executor lib | 1,254 passed, 0 failed, 14 ignored; 156.43초. Rust test-harness만 직렬 실행, solver Auto 정책 유지. |
| 마지막 audit 수정 후 `solution_set_audit` 전체 module | 20 passed, 0 failed. P7P7 크기의 dense/sparse/empty 입력과 정확한 cap/1 byte 부족 경계 등 새 회귀 3개 포함. |
| 최종 source의 `wasm32-unknown-unknown` clearra-app check | 통과. |
| 최종 native release 빌드 | 통과. |
| 최종 전체 P7P7 실행 | OFF / ON / 진단 없는 기본 설정 모두 완주, 위 identity 일치. |

마지막 audit 수정 뒤 전체 core suite를 다시 실행했다고 기록하지 않는다. 해당
module 전체 회귀, 최종 Wasm 컴파일, 실제 전체 제품 요청의 세 완료 실행으로
남은 변경을 검증했다. native 시간은 이 환경의 Auto/9 결과이며 브라우저/Wasm의
실행 시간을 측정한 것은 아니다.

### 10.5. 재현과 자료 위치

저장소 root에서 잠긴 의존성으로 빌드한다. 아래 source stamp는 **실제로 빌드하는
checkout**을 기록하므로, 본 절의 측정값을 재현하려면 먼저 측정 commit을 사용한다.
문서만 추가된 후속 commit에서 빌드하면 그 commit이 별도로 기록된다.

```sh
CLEARRA_SOURCE_COMMIT="$(git rev-parse HEAD)" cargo build --locked --release \
  --no-default-features -p clearra-app --example oracle_ctk3_bench \
  --features parallel,local-search-ab,wasm-stage-profiling
cargo build --locked --release -p clearra-manage

build/cargo/default/release/clearra-manage runtime run \
  --producer oracle-p7p7-default --profile benchmark-search --timeout 300 -- \
  build/cargo/default/release/examples/oracle_ctk3_bench run
```

동일 바이너리 A/B는 위 manager 앞에 `CLEARRA_EXTENDED_HARDDROP_PROOF=0` 또는
`CLEARRA_EXTENDED_HARDDROP_PROOF=1`을 설정하고, example의 `run` 뒤에 `--profile`과
`--cancel-after-seconds 300`을 추가한다. 이때 manager timeout은 330초를 사용한다.
다른 local override는 설정하지 않는다. 하네스의 300초 deadline은 비교 실행의
진단 보호 장치이며 세 완료 결과에서 cancellation을 발생시키지 않았다.

원시 자료는 `_local/artifacts/oracle-p7p7-resumed-20261122/`에 둔다.
`final-off.json`, `final-on.json`, `product-default.json`과 각각의 stderr/receipt,
`completion-parity.json`이 이번 완료 증거다. 단위 회귀는
`_local/artifacts/harddrop-proof-20261122/`에, 최종 빌드·Wasm check 로그는 완료
자료 디렉터리에 보존한다. 이전 incomplete 진단 파일은 이름과 상태를 유지했다.
