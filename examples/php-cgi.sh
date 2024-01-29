#!/bin/sh

echo "$SERVER_PROTOCOL 200 OK"
echo "Date: $(date)"
echo "Server: $SERVER_NAME"
echo "Content-Type: text/html"
echo
echo
cat <<EOF
<!DOCTYPE html>
<html>
<head>
<title>$SERVER_SOFTWARE</title>
</head>
<body>
<p>Should run php-cgi with script <pre>$1</pre></p>
<p>But php-cgi was not found.</p>
</body>
</html>
EOF
