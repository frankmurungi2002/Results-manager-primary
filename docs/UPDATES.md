# Automatic updates

Every installed copy of Termbook checks once a day (only when the school
allows the internet) for a newer signed release in the public repository
`frankmurungi2002/termbook-releases`. The source code stays private. When one exists, the School Admin sees it on the dashboard and in
Settings → Internet & updates, and presses **Install**: Termbook takes a
backup, downloads the installer, checks its signature, installs it and
restarts. Nothing is ever installed without a valid signature, and a schoo## One-time setup (the owner)

1. Run `owner\1 Set up my keys (once).bat`. It makes `updater.key` in
   `%USERPROFILE%\TermbookKeys` (no password: the file itself is the
   secret, so guard it like the licence key) and writes its public half into
   `src-tauri/updater.pub` and `tauri.conf.json`. Commit and push.
2. On GitHub, create a **public** repository named `termbook-releases`
   (empty, with a README). Schools download installers from it; it never
   holds code.
3. Make a token that may publish there: GitHub → Settings → Developer
   settings → Fine-grained tokens → Generate: repository access *Only
   termbook-releases*, permission *Contents: Read and write*.
4. In the source repository: Settings → Secrets and variables → Actions → New
   repository secret, twice:
   * `TAURI_SIGNING_PRIVATE_KEY`: everything inside `updater.key`
     (open it in Notepad, copy all);
   * `RELEASES_TOKEN`: the token from step 3.

  * `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: its password

## Releasing a version

1. Raise the version in three places, the same number:
   `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`.
2. Commit, then tag and push the tag:

       git tag v0.2.0
       git push origin v0.2.0

3. The Release workflow builds the installer, signs it and publishes the
   release with `latest.json`. Within a day every school with internet sees
   the update.

Schools already running a build without the public key need one manual
install of a build that has it; after that, updates are automatic.
