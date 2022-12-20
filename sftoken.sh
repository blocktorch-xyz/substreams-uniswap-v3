export FIREHOSE_API_TOKEN=$(curl https://auth.streamingfast.io/v1/auth/issue -s --data-binary '{"api_key":"'$STREAMINGFAST_KEY'"}' | jq -r .token)
export SUBSTREAMS_API_TOKEN=$FIREHOSE_API_TOKEN

if [ -z $SUBSTREAMS_API_TOKEN ]; then
    echo Auth token is NOT set, check https://app.streamingfast.io/ health;
else
    echo Auth token set on FIREHOSE_API_TOKEN and SUBSTREAMS_API_TOKEN;
fi
