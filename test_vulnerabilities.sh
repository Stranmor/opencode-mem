curl -s -X POST 'http://localhost:37777/save' \
-H 'Content-Type: application/json' \
-d '{"title":"TEST","narrative":"TEST","project":null,"observation_type":"bugfix","noise_level":"low","facts":["string1", "string2"],"concepts":[]}'
echo ""
