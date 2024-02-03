#!/bin/sh

#echo "$SERVER_PROTOCOL 200 OK"
echo "Date: $(date)"
echo "Server: $SERVER_NAME"
echo "Content-Type: text/html"
echo "Content-Length: $CONTENT_LENGTH"
echo
cat <<EOF
<!DOCTYPE html>
<html lang="en">
<head>
<title>$SERVER_SOFTWARE</title>
</head>
<body>
<p>Should run php-cgi with script <code>$1</code></p>
<p>But php-cgi was not found.</p>
</body>
</html>

EOF
