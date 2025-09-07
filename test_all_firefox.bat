@echo off
echo Testing all Firefox processes...

REM Liste des PIDs Firefox trouvés
set PIDS=23664 24304 25276 34000 21800 33080 15284 21844 24376 9312 34528 16356 27912 32104 29864 25452 11900 38728 44152 40792 13884 31516 23932 42428 28272 42980 36480 2128 9736 5528

for %%p in (%PIDS%) do (
    echo.
    echo ========================================
    echo Testing Firefox PID: %%p
    echo ========================================
    cargo run --release %%p
    echo.
)

echo All Firefox processes tested!
pause
