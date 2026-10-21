# Rust (Axum + SQLite) 단일 바이너리 아키텍처 채택

오라클 클라우드(OCI) 무료 티어 Ubuntu 인스턴스(`english-note.duckdns.org`, Nginx 리버스 프록시 + `systemd`)의 제한된 메모리/CPU 자원 안에서 빠르고 안정적으로 구동하기 위해, Node.js/Next.js 대신 기존 `English_note`와 동일한 **Rust (Axum + SQLx SQLite + Askama/Vanilla JS)** 단일 바이너리 아키텍처를 사용한다. 별도의 DB 서버 프로세스 없이 SQLite(`WAL` 모드) 파일 하나로 모든 학습 카드와 세션을 관리하며, 원클릭 백업 및 기존 `notes.db` 마이그레이션을 기본 지원한다.
