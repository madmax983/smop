- `env::var` reported a value that is not valid UTF-8 as "not set" (#36).
  Now the error is `Environment variable X is not valid UTF-8: "80\xFF80"`.
