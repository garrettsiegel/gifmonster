# Node Release Runbook

## Preconditions
- Clean working tree (no modified, staged, or untracked files)
- Target version updated across package manifests:
	- `gifmonster-node/package.json`
	- `gifmonster-node/package-lock.json`
	- `gifmonster-node/npm/*/package.json`
- CI green on main

## Local Verification
1. `cd gifmonster-node`
2. `npm ci`
3. `npm test`
4. `npm run release:verify`
5. `npm run pack:dry-run` (must complete without errors; validates package contents without publishing)

## Publish Trigger
- Create and push tag `vX.Y.Z`:
	- `git tag vX.Y.Z`
	- `git push origin vX.Y.Z`
- Publish a GitHub Release for that tag (or run the publish workflow manually with `workflow_dispatch`)
- Publish workflow verifies tag/package version and release readiness before npm publish

## Post-Publish Verification
- Verify published packages are available on npm:
	- `npm view gifmonster version --json`
	- `npm view gifmonster-darwin-arm64 version --json`
	- `npm view gifmonster-darwin-x64 version --json`
	- `npm view gifmonster-linux-x64-gnu version --json`
	- `npm view gifmonster-linux-x64-musl version --json`
	- `npm view gifmonster-win32-x64-msvc version --json`
