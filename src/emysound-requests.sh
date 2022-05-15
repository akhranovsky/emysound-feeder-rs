#curl --request POST \
#  --url http://localhost:3340/api/v1.1/Tracks \
#  --header 'Accept: application/json' \
#  --header 'Authorization: Basic QURNSU46' \
#  --header 'Content-Type: multipart/form-data' \
#  --form file='@testdata/Label_01.mp3' \
#  --form Id=id \
#  --form Title=title \
#  --form Artist=artist \
#  --form newKey=New%20Value \
#  --form newKey-1=New%20Value \
#  --form MediaType=Audio

# curl --request POST \
#      --url 'http://localhost:3340/api/v1.1/Query?mediaType=Audio&minConfidence=0.2&minCoverage=0&registerMatches=true' \
#      --header 'Accept: application/json' \
#      --header 'Authorization: Basic QURNSU46' \
#      --header 'Content-Type: multipart/form-data' \
#      --form file='@testdata/Label_10.mp3

ids=`curl --request GET \
     --url 'http://localhost:3340/api/v1.1/Tracks?offset=0' \
     --header 'Accept: application/json' \
     --header 'Authorization: Basic QURNSU46' \
| jq -r '.results[].id'`

for id in $ids; do
     curl --request DELETE \
     --url http://localhost:3340/api/v1.1/Tracks/$id \
     --header 'Accept: application/json' \
     --header 'Authorization: Basic QURNSU46'
done;
