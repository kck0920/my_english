# My English

나만의 영어 청크(Chunk) 수집, AI 영작·첨삭 및 간격 반복(SRS) 체화 학습 앱.

## Language

**Chunk**:
원어민이 하나의 의미 단위로 묶어서 사용하는 어구, 구동사, 콜로케이션 또는 패턴 (예: `get me every time`, `be supposed to`). 이 앱에서 학습하고 암기하는 핵심 타겟 단위이다.
_Avoid_: Word, vocabulary, idiom, phrase

**Context Sentence**:
**Study Card**에서 타겟 **Chunk**가 실제로 어떻게 쓰이는지 보여주는 대표 영어 예문. **Review** 시에는 이 문장 안의 **Chunk**(실제 활용형 포함) 자리가 빈칸(Cloze)으로 가려져 출제된다.
_Avoid_: Example sentence, main sentence, prompt sentence

**Study Card**:
하나의 **Chunk**를 실제 문맥 속에서 학습하기 위해 저장된 카드. 타겟 **Chunk**(원형), 대표 예문(**Context Sentence** 및 문장 내 실제 활용형 **Surface Chunk**), 한국어 의미, 추가 응용 예문(1~2개), AI 뉘앙스/교정 해설, 상황 태그(Tag), 그리고 현재 **SRS Stage**를 포함한다.
_Avoid_: Flashcard, note, item, deck item

**Draft Card**:
**Smart Input**에 한국어 질문, 어색한 영작, 또는 외부 영어 지문을 입력했을 때 AI가 생성·추출하여 제안하는 **Study Card** 후보. 사용자가 인라인으로 수정하거나 선택·확정하면 **Study Card**로 저장된다.
_Avoid_: Suggestion, candidate, temp card

**Smart Input**:
한국어 질문("이거 영어로 뭐라고 해?"), 내 영작 문장(교정 요청), 또는 외부 영어 지문(아티클/자막)을 구분 없이 한 곳에 입력하면 AI가 의도를 판별해 **Draft Card** 목록으로 변환해 주는 통합 입력창.
_Avoid_: Search bar, chat box, input form

**Review**:
간격 반복(SRS) 스케줄에 따라 도래한(Due) **Study Card**의 한국어 의미와 빈칸 처리된 **Context Sentence**를 보고 타겟 **Chunk**를 능동적으로 인출(타이핑 또는 터치 확인)한 뒤 기억 난이도(`Again` / `Good` / `Easy`)를 평가하는 학습 행위.
_Avoid_: Quiz, test, exam, study session

**SRS Stage**:
**Study Card**의 기억 정착도를 나타내는 고정 복습 단계 (`Stage 0: 학습 전/당일 재복습` → `Stage 1: 1일` → `Stage 2: 3일` → `Stage 3: 7일` → `Stage 4: 14일` → `Stage 5: 30일`). **Review** 평가 결과에 따라 `Again`이면 Stage 1(내일)로 리셋 및 당일 재출제되고, `Good`이면 1단계 상승, `Easy`이면 2단계 상승한다.
_Avoid_: Box, level, ease factor, interval

**Output Transfer**:
**SRS Stage**가 3단계(7일) 이상인 **Study Card**를 **Review**할 때, 기존 **Context Sentence**를 단순 암기했는지 검증하기 위해 AI가 동일한 **Chunk**를 사용하는 새로운 한국어 상황 문장을 즉석 출제하고 사용자의 응용 영작을 피드백하는 심화 복습 모드.
_Avoid_: Speed review, random test, free writing

**Audio Gym**:
화면을 보지 않는 이동/운동 중에 선택한 **Study Card**들을 **"한국어 의미 TTS → 3초 침묵(직접 말하기 시간) → 원어민 영어 Context Sentence TTS 2회"** 순서로 연속 재생하는 핸즈프리 스피킹 훈련 모드.
_Avoid_: Continuous listening, audio player, playlist
