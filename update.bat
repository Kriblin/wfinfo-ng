@echo off
curl https://api.warframestat.us/wfinfo/prices/ | jq . > prices.json
curl https://api.warframestat.us/wfinfo/filtered_items/ | jq . > filtered_items.json
if not exist tessdata mkdir tessdata
curl -L https://github.com/tesseract-ocr/tessdata_fast/raw/main/eng.traineddata -o tessdata/eng.traineddata
