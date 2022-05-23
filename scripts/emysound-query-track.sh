curl --request POST \
     --url 'http://localhost:3340/api/v1.1/Query?mediaType=Audio&minConfidence=0.2&minCoverage=0' \
     --header 'Accept: application/json' \
     --header 'Authorization: Basic QURNSU46' \
     --header 'Content-Type: multipart/form-data' \
     --form file=@$1 \
     | jq ".[] | {artist: .track.artist, title: .track.title, coverage: .audio.coverage}"
