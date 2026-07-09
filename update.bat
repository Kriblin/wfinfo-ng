@echo off
if not exist tessdata mkdir tessdata
curl -L https://github.com/tesseract-ocr/tessdata_fast/raw/main/eng.traineddata -o tessdata/eng.traineddata
