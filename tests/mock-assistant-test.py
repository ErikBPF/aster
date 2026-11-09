"""Fixture contract: both instruction and reference must arrive in system context."""
import runpy
from pathlib import Path
mock = runpy.run_path(str(Path(__file__).parent / 'fixtures/mock-assistant.py'))
for text in ['', 'ASTER_DEMO_READ_ONLY_V1', 'ASTER_DEMO_ORDERS_V1']:
    assert '```sql' not in mock['reply']({'model': 'aster-mock-assistant', 'messages': [{'role': 'system', 'content': text}]})['choices'][0]['message']['content']
text = 'ASTER_DEMO_READ_ONLY_V1 ASTER_DEMO_ORDERS_V1'
assert mock['SQL'] in mock['reply']({'model': 'aster-mock-assistant', 'messages': [{'role': 'system', 'content': text}]})['choices'][0]['message']['content']
assert '```sql' not in mock['reply']({'model': 'aster-mock-assistant', 'messages': [{'role': 'user', 'content': text}]})['choices'][0]['message']['content']
print('MOCK_ASSISTANT_FIXTURE_OK')
