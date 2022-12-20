export FIREHOSE_API_TOKEN=$(curl https://auth.streamingfast.io/v1/auth/issue -s --data-binary '{"api_key":"'$GITPOD_STREAMINGFAST_KEY'"}' | jq -r .token)
export SUBSTREAMS_API_TOKEN=$FIREHOSE_API_TOKEN
echo Token set on FIREHOSE_API_TOKEN and SUBSTREAMS_API_TOKEN
