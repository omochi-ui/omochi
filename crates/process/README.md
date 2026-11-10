# Omochi process ownership

Shared native command execution for CLI development/compositions and utilities.
Unix children lead a process group. Windows children enter a kill-on-close Job
Object atomically through `PROC_THREAD_ATTRIBUTE_JOB_LIST`; only selected stdio
handles are inherited. Arguments are passed without a shell. Windows batch files
require an explicit shell command.

`Group::signal(SIGTERM)` requests termination (CTRL_BREAK on Windows); callers wait
one second before `SIGKILL` terminates the whole tree. Retaining a cloned group
retains ownership, and dropping a child kills its tree. `ShutdownSignal` registers
console/signal handlers before publication. A Windows native process launched by
the npm stub also watches the original launcher handle, so TerminateProcess of
the stub cannot leave a service running.
