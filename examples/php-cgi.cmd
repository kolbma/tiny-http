@echo off
REM echo %SERVER_PROTOCOL% 200 OK
echo Date: %DATE%
echo Server: %SERVER_NAME%
echo Content-Type: text/html
echo Content-Length: %CONTENT_LENGTH%
echo.
echo ^<!DOCTYPE html^>
echo ^<html lang="en"^>
echo ^<head^>
echo ^<title^>%SERVER_SOFTWARE%^</title^>
echo ^</head^>
echo ^<body^>
echo ^<p^>Should run php-cgi with script ^<code^>%1^</code^>^</p^>
echo ^<p^>But php-cgi was not found.^</p^>
echo ^</body^>
echo ^</html^>
echo.
