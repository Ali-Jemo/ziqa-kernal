#!/bin/busybox hush
# POSIX Signal/Job Control Verification Script

echo ">>> [hush_test] Starting Signal/Job Control Test"

# 1. Test setpgid
/bin/busybox echo ">>> [hush_test] Testing setpgid..."
# In hush, we can use builtins or run commands. 
# A simple way to test is to spawn a background job and see if it runs.
/bin/busybox sleep 2 &
PID=
echo ">>> [hush_test] Spawned background job PID="
/bin/busybox kill -TERM 
echo ">>> [hush_test] Sent SIGTERM to PID="

# 2. Test wait (relies on SIGCHLD)
echo ">>> [hush_test] Testing wait (SIGCHLD)..."
/bin/busybox ls > /dev/null
/bin/busybox wait
echo ">>> [hush_test] Wait finished."

echo ">>> [hush_test] Test complete."
