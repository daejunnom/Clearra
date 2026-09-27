# v0.8.1 Web 자산 상태 파일의 손상 복구

## 실제 결함과 수정

기준은 `codex/v081-selective-source-ci-20260927`의
`6f01d4460be8f229b8c91ec1606b6fbdca1022d5`다. OPFS 상태 파일 `active.json`이
문법상 유효한 JSON `null`이면 기존 parser가 객체라고 가정해 `schema`를 읽으며
`TypeError`를 냈다. download worker는 예상된 포인터/길이/digest 오류만
손상 자산 상태로 분류하므로 이 경우 signed plan과 재다운로드·삭제 안내 대신
일반 실패만 전달했다.

parser는 JSON을 `unknown`으로 받아 객체 여부를 확인한 뒤에만 기존 필드,
길이·identity·파일 이름 검증을 수행한다. `null`, 원시 값과 배열은 같은
`accelerator_store_pointer_invalid`로 거절한다. worker의 기존 복구 오류 분류를
작은 독립 모듈로 옮겨 실제 store 오류가 복구 UI 경계까지 같은 의미로 전달되는지
검사한다. 예상 밖의 `TypeError`, store busy, quota 오류를 손상으로 숨기지 않는다.

명시 consent, 서명 catalog, generation/용량/파일 이름 제한과 search worker의
invalid asset 제거 후 exact fallback은 변경하지 않는다. TB와 namespace를
혼합하거나 동의 없는 원격 download를 추가하지 않는다.

## 좁은 기능 증거

- 수정 전에 추가한 `null` 회귀 검사는 실제 production store의
  `Cannot read properties of null (reading 'schema')`로 실패했다.
- 수정 후 잘못된 JSON 값 9종의 status/read/warm-identity 검사 27개가 모두
  같은 손상 코드로 거절된다. 실제 production repair classifier도 true를 반환한다.
- 같은 in-memory OPFS 계약에서 재저장한 pointer/digest/current 상태와 삭제 후
  empty 상태를 대조했다. 이전 staged/pointer 취소, missing payload와 digest 손상
  회귀도 계속 통과했다. browser profile이나 실제 자산 자료를 생성하지 않는다.
- 변경된 TypeScript 계약을 output-file 없는 in-memory bundle로 실행했고,
  `@clearra/web`의 `tsconfig.contract.json` 타입 검사도 통과했다.

이것은 production 함수와 모의 OPFS의 집중 기능 증거다. 실제 브라우저 파일
시스템·cross-tab locks·화면·clipboard 검증은 아니다. 브라우저 제어 연결을
재시도했지만 초기화 단계의 OS path-not-found 오류로 실행되지 않았으며 기존
4194 서버와 사용자 감사 세션을 교체하지 않았다. 브라우저 검증은 Open이다.

## CI와 다음 권위

현재 `6f01d446…`의 비게시 [run 36350733063](https://github.com/daejunnom/Clearra/actions/runs/36350733063)은
진행 중이다. 그 실행에는 이 후속 pointer 수정이 없다. 종료 전 push로 취소하지
않으며 이 수정의 CI 검증은 별도다. 기존 surfaces job의 전체 Web 계약 발견에
변경한 target이 이미 포함되므로 새 build/download job을 추가하지 않는다.

벤치마크·ABBA, 자료 재생성/재자격, v0.9.0, main 병합과 배포는 하지 않았다.

## 후속: 취소와 포인터 기록 완료가 겹치는 경계

명시 다운로드의 OPFS `active.json`을 `close()`하는 동안 사용자가 취소하면,
기존 코드는 `close()` 반환 뒤 취소 상태를 재검사하지 않았다. 포인터가 이미
새 세대로 바뀐 뒤에도 취소가 도착할 수 있어, 이전 자산 보존 계약을 원자적
경계에서 확인해야 했다. 배타 lock을 보유한 상태에서 이전 포인터의 정확한
바이트를 보관하고, 취소가 기록 완료와 겹치면 이전 포인터를 복원한 뒤 새
staging 파일을 제거한다. 복원에 실패하면 성공한 취소로 보고하지 않고
`accelerator_store_rollback_failed`를 유지한다. 비취소 `close()` 실패가 새
포인터를 게시했거나, 게시 후 검증 readback이 실패하면
`accelerator_store_commit_uncertain`으로 표시하고 명시 상태 재확인을 요구한다.
기존 포인터의 복원 snapshot은 64KiB로 제한한다. 이보다 큰 손상 파일은
덮어쓰기 전에 `accelerator_store_pointer_requires_remove`로 거절하고, 사용자가
먼저 명시 삭제한 뒤 다시 설치하도록 안내한다.

실제 download worker는 취소 신호가 있어도 두 위험 코드를 일반 취소로
덮어쓰지 않는다. GUI는 그동안 이전 자산 보존을 단정하지 않고 상태 재확인
전 다운로드·삭제를 막는다. worker 자체가 비정상 종료된 뒤에도 상태 확인으로
새 worker를 시작할 수 있다. 이는 TB namespace나 자동 다운로드 정책을 바꾸지
않는다.

집중 검증은 production store와 메모리 OPFS 모형에서 기존 세대·첫 설치·손상된
작은 포인터의 `close()` 취소, 64KiB 초과 포인터의 사전 거절, 게시 후 close
오류, rollback 실패를 각각 실행했다.
독립 오류 코드 경계도 검사했고, Node 직접 실행·Web 계약 타입 검사·Svelte
구문 컴파일이 통과했다. 현재 branch의 실제 브라우저 OPFS, 파일 snapshot,
cross-tab lock, 사용자 화면 readback 및 새 수정의 CI는 아직 `Open`이다.
