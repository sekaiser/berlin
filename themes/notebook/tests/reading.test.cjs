const {test} = require('node:test');
const assert = require('node:assert/strict');
const {matchingReferences, normalizeReading} = require('../shared/static/js/reading.js');
const references = [
  {text: normalizeReading('Storage design: a paper'), tags:['storage','design']},
  {text: normalizeReading('Storage repository'), tags:['storage']},
  {text: normalizeReading('Design notes'), tags:['design']},
];
test('reading filters intersect every selected tag and search word', () => {
  assert.deepEqual(matchingReferences(references,'',['storage','design']),[references[0]]);
  assert.deepEqual(matchingReferences(references,'repository',['storage','design']),[]);
  assert.deepEqual(matchingReferences(references,'STORAGE REPOSITORY'.toLowerCase(),['storage']),[references[1]]);
  assert.deepEqual(matchingReferences(references,'',[]),references);
});
