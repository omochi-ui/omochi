#ifndef RUNNER_IDENTITY_LEASE_H_
#define RUNNER_IDENTITY_LEASE_H_

#include <windows.h>

// Creates (but does not acquire) the same mutex for this Windows user in every
// login session. Returns nullptr on failure; callers must not start unlocked.
HANDLE CreateIdentityMutex();

#endif  // RUNNER_IDENTITY_LEASE_H_
