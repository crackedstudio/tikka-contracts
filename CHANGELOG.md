# Changelog

All notable changes to this project are documented here.  
Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)  
Versioning: [Semantic Versioning](https://semver.org/)

## [Unreleased]

### Added
- Architecture documentation with factory -> instance -> oracle flow and state-machine diagrams (`docs/ARCHITECTURE.md`).
- Comprehensive rustdoc comments for all public `raffle-shared` enums, structs, fields, constants, and functions.
- Pull request template requiring changelog updates for non-trivial changes.

### Changed
- README documentation section now links to architecture docs.

### Documented
- Standardized event emission model and event catalog (`docs/EVENTS.md`).
- Lifecycle/admin event coverage and event publishing patterns from the previous implementation summary.
- Admin key migration was recorded as a historical note (source file existed but contained no additional details).

## [0.2.0] - 2025-01-01

### Added
- Commit-reveal randomness source.
- Max tickets per transaction cap.
- Claim lockup delay configuration.
- Drawing/finalization guard state.

### Fixed
- Admin zero-address validation in `set_admin`.
- Duplicate winner selection in oracle finalization path.
