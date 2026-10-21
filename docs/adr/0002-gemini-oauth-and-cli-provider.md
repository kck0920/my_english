# 브라우저 기반 Google OAuth (Cloud Code Assist API) 및 로컬 Antigravity CLI 듀얼 AI 프로바이더

별도의 유료 종량제 API 키 과금 없이 사용자가 보유한 Gemini Pro 권한을 웹앱에서 그대로 활용할 수 있도록, AI 호출 계층은 다음 두 가지 프로바이더를 어댑터 패턴으로 지원한다:

1. **Google OAuth PKCE 브라우저 인증 (Cloud Code Assist API, 기본값)**: 웹앱 설정 화면에서 Google OAuth PKCE 인증 URL(`https://codeassist.google.com/authcode`)을 통해 로그인 코드를 입력하면, 리프레시 토큰을 암호화/로컬 저장하고 `cloudcode-pa.googleapis.com/v1internal:generateContent` 엔드포인트를 통해 Gemini Pro 모델을 직접 호출한다.
2. **로컬 Antigravity CLI (`agy`) / Gemini CLI 폴백**: 로컬 개발 환경이나 CLI가 설치된 환경에서는 `agy -p --output-format json --json-schema` 비대화형 서브프로세스 호출도 선택적으로 사용할 수 있도록 지원한다.
