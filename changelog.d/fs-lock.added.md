- `fs::lock(path, timeout)` and `fs::lock_shared(path, timeout)` (#33):
  file locks that work together with `flock(1)`, held by a `fs::FileLock`
  guard until it drops. While another holder has the lock they retry every
  50 ms until `timeout` (`Duration::ZERO` tries once, `Duration::MAX` waits
  indefinitely), then fail naming the path and the timeout. Like `flock(1)`,
  a missing path is created as an empty file, and an exclusive lock opens
  the file for writing when it can (NFS needs that). On Unix the lock is advisory
  and a directory can be locked too. On Windows the lock is mandatory (other
  handles can't write the file, or read it under an exclusive lock) and
  locking a directory is an error, so cross-platform code locks a dedicated
  lock file.
