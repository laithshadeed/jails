" Compiler: jails (Maven diagnostics, driven through `jails check`)
if exists("current_compiler")
  finish
endif
let current_compiler = "jails"

CompilerSet makeprg=jails\ check
CompilerSet errorformat=[FATAL]\ Non-parseable\ POM\ %f:\ %m%\\s%\\+@%.%#line\ %l\\,\ column\ %c%.%#,
CompilerSet errorformat+=[%tRROR]\ Malformed\ POM\ %f:\ %m%\\s%\\+@%.%#line\ %l\\,\ column\ %c%.%#,
CompilerSet errorformat+=[FATAL]\ Non-parseable\ POM\ %f:\ %m%\\s%\\+%.%#@%l:%c%.%#,
CompilerSet errorformat+=[%tRROR]\ Malformed\ POM\ %f:\ %m%\\s%\\+%.%#@%l:%c%.%#,
CompilerSet errorformat+=[%tARNING]\ %f:[%l\\,%c]\ %m
CompilerSet errorformat+=[%tRROR]\ %f:[%l\\,%c]\ %m
CompilerSet errorformat+=%A[%t%[A-Z]%#]\ %f:[%l\\,%c]\ %m,%Z
CompilerSet errorformat+=%A%f:[%l\\,%c]\ %m,%Z
CompilerSet errorformat+=[%tARNING]\ %f:[%l]\ %m
CompilerSet errorformat+=[%tRROR]\ %f:[%l]\ %m
CompilerSet errorformat+=%A[%t%[A-Z]%#]\ %f:[%l]\ %m,%Z
CompilerSet errorformat+=%A%f:[%l]\ %m,%Z
CompilerSet errorformat+=[%tARNING]\ %f:%l:%c:\ %m
CompilerSet errorformat+=[%tRROR]\ %f:%l:%c:\ %m
CompilerSet errorformat+=%A[%t%[A-Z]%#]\ %f:%l:%c:\ %m,%Z
CompilerSet errorformat+=%A%f:%l:%c:\ %m,%Z
CompilerSet errorformat+=[%tARNING]\ %f:%l:\ %m
CompilerSet errorformat+=[%tRROR]\ %f:%l:\ %m
CompilerSet errorformat+=%A[%t%[A-Z]%#]\ %f:%l:\ %m,%Z
CompilerSet errorformat+=%A%f:%l:\ %m,%Z
CompilerSet errorformat+=%+E\ \ %#test%m,%Z
CompilerSet errorformat+=%+E[ERROR]\ Please\ refer\ to\ %f\ for\ the\ individual\ test\ results.
CompilerSet errorformat+=%+E%>[ERROR]\ %.%\\+Time\ elapsed:%.%\\+<<<\ FAILURE!,
CompilerSet errorformat+=%+E%>[ERROR]\ %.%\\+Time\ elapsed:%.%\\+<<<\ ERROR!,
CompilerSet errorformat+=%+Z%\\s%#at\ %f(%\\f%\\+:%l),
CompilerSet errorformat+=%+C%.%#
CompilerSet errorformat+=%-GAudit\ done.,
CompilerSet errorformat+=%-G[INFO]\ %.%#,
CompilerSet errorformat+=%-G[debug]\ %.%#
