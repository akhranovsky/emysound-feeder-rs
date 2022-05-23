curl --request POST \
  --url http://localhost:3340/api/v1.1/Tracks \
  --header 'Accept: application/json' \
  --header 'Authorization: Basic QURNSU46' \
  --header 'Content-Type: multipart/form-data' \
  --form file=@$1 \
  --form Id=`uuid -v4` \
  --form Title=$3 \
  --form Artist=$2 \
  --form MediaType=Audio
