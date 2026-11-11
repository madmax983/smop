- `env::is_ci()` (#36): `true` when `CI` is set and is not `false` or `0`.
  When `CI` is unset or empty, it reads `TF_BUILD`, `JENKINS_URL` with
  `BUILD_ID`, `TEAMCITY_VERSION` and `CODEBUILD_BUILD_ID`.
