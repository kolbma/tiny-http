@echo off
echo %SERVER_PROTOCOL% 200 OK
echo Date: %DATE%
echo Server: %SERVER_NAME%
echo Content-Type: text/html
echo.
echo.
echo ^<!DOCTYPE html^>
echo ^<html^>
echo ^<head^>
echo ^<title^>%SERVER_SOFTWARE%^</title^>
echo ^</head^>
echo ^<body^>
echo ^<p^>Should run php-cgi with script ^<pre^>%1^</pre^>^</p^>
echo ^<p^>But php-cgi was not found.^</p^>
echo ^</body^>
echo ^</html^>
