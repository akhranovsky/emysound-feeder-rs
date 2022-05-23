ids=`curl --request GET \
     --url 'http://localhost:3340/api/v1.1/Tracks?offset=0' \
     --header 'Accept: application/json' \
     --header 'Authorization: Basic QURNSU46' \
| jq -r '.results[].id'`

for id in $ids; do
     curl -q --request DELETE \
     --url http://localhost:3340/api/v1.1/Tracks/$id \
     --header 'Accept: application/json' \
     --header 'Authorization: Basic QURNSU46'
done;
