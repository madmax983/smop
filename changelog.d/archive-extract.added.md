- `archive::extract(archive, dest)` selects `extract_zip`, `extract_tar` or
  `extract_tar_gz` from the end of the file name: `.zip`, `.tar`, `.tar.gz`
  or `.tgz`, in any letter case. A different end is an error that lists the
  supported extensions (ADR 0020).
