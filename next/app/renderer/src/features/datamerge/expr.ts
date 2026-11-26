import {tr} from '../../i18n';
import type {ExprError} from './query';

/** The sentence an expression error says: the engine gives `code` and `args` (stable), the words are the screen's. */
export function errorText(e:ExprError):string {
  const a=e.args;
  const wanted=(w:string)=>w==='a value'?tr('값'):w==='an operator'?tr('연산자'):w;
  const count=(w:string)=>{
    const range=/^(\d+) to (\d+)$/.exec(w),more=/^(\d+) or more$/.exec(w);
    return range?tr('{0}~{1}개',{0:range[1],1:range[2]}):more?tr('{0}개 이상',{0:more[1]}):tr('{0}개',{0:w});
  };
  switch(e.code){
    case 'empty': return tr('식이 비어 있습니다.');
    case 'tooLong': return tr('식이 너무 깁니다 (4000자까지).');
    case 'tooDeep': return tr('괄호나 함수가 너무 깊게 겹쳤습니다.');
    case 'unexpectedChar': return tr('쓸 수 없는 글자 {0} 입니다.',{0:a[0]});
    case 'unterminatedString': return tr('따옴표로 연 글이 닫히지 않았습니다.');
    case 'unterminatedField': return tr('중괄호로 연 필드 이름이 닫히지 않았습니다.');
    case 'emptyField': return tr('중괄호 안에 필드 이름이 없습니다.');
    case 'badNumber': return tr('{0} 은(는) 올바른 숫자가 아닙니다.',{0:a[0]});
    case 'unexpectedToken': return tr('여기에는 {0} 을(를) 쓸 수 없습니다.',{0:a[0]});
    case 'unexpectedEnd': return tr('식이 끝나기 전에 {0} 이(가) 필요합니다.',{0:wanted(a[0])});
    case 'unknownFunction': return tr('{0} 이라는 함수는 없습니다.',{0:a[0]});
    case 'unknownName': return tr('{0} 은(는) 알 수 없는 이름입니다. 필드 이름은 중괄호로, 글은 따옴표로 감싸세요.',{0:a[0]});
    case 'unknownField': return tr('데이터 원본에 {0} 필드가 없습니다.',{0:a[0]});
    case 'laterField': return tr('계산 열 {0} 은(는) 이 열보다 아래에 있습니다. 계산 열은 위에 있는 필드만 쓸 수 있습니다.',{0:a[0]});
    case 'argCount': return tr('{0} 함수에 필요한 값은 {1}입니다. 지금은 {2}개를 적었습니다.',{0:a[0],1:count(a[1]),2:a[2]});
    case 'badName': return tr('필드 이름이 비었거나 중괄호·제어 문자를 포함합니다.');
    case 'duplicateName': return tr('같은 이름의 필드가 이미 있습니다.');
    case 'tooManyFields': return tr('계산 열은 64개까지 만들 수 있습니다.');
    case 'notNumber': return tr('{0} 은(는) 숫자가 아닙니다.',{0:a[0]});
    case 'notDate': return tr('{0} 은(는) 날짜가 아닙니다. yyyy-mm-dd, yyyy/mm/dd, yyyy.mm.dd 로 쓰세요.',{0:a[0]});
    case 'notBool': return tr('{0} 은(는) TRUE 나 FALSE 가 아닙니다.',{0:a[0]});
    case 'divideByZero': return tr('0으로 나눌 수 없습니다.');
    case 'overflow': return tr('숫자가 너무 큽니다.');
    case 'badArgument': return tr('{0} 함수에 넣은 값이 맞지 않습니다: {1}',{0:a[0],1:a[1]});
    case 'badRegex': return tr('정규식이 올바르지 않습니다: {0}',{0:a[0]});
    case 'noMatch': return tr('IFS 의 조건이 하나도 참이 아닙니다.');
    case 'tooLongResult': return tr('결과가 너무 깁니다.');
    default: return e.message;
  }
}

/** The explanation under an expression's box: where the mistake is (from the character it starts at), and what it is. */
export function errorWithPlace(e:ExprError,expression:boolean):string {
  const text=errorText(e);
  return expression&&e.code!=='badName'&&e.code!=='duplicateName'&&e.code!=='tooManyFields'?tr('{0}번째 글자부터: {1}',{0:e.pos+1,1:text}):text;
}

/** The parentheses of an expression (the ones inside a text or a field name do not count): how many there are, whether they pair up,
 *  and the partner of the one beside the caret. */
export function bracketInfo(text:string,caret:number):{open:number;close:number;balanced:boolean;partner:number|null} {
  const pair=new Map<number,number>(),stack:number[]=[];
  let open=0,close=0,unmatched=0,quoted=false,field=false;
  for(let i=0;i<text.length;i++){
    const c=text[i];
    if(quoted){if(c==='"'){if(text[i+1]==='"')i++;else quoted=false;}continue;}
    if(field){if(c==='}')field=false;continue;}
    if(c==='"')quoted=true;
    else if(c==='{')field=true;
    else if(c==='('){open++;stack.push(i);}
    else if(c===')'){close++;const o=stack.pop();if(o===undefined)unmatched++;else{pair.set(o,i);pair.set(i,o);}}
  }
  const beside=[caret-1,caret].find(i=>i>=0&&pair.has(i));
  return {open,close,balanced:stack.length===0&&unmatched===0,partner:beside===undefined?null:pair.get(beside)??null};
}

/** What a function does, one line each (the engine gives the names and the argument counts). */
export function functionHelp(name:string):string {
  switch(name){
    case 'CONCAT': return tr('글을 차례로 이어 붙입니다.');
    case 'TEXTJOIN': return tr('글을 구분자로 이어 붙입니다. 두 번째 값이 TRUE 이면 빈 값은 건너뜁니다.');
    case 'LEFT': return tr('글의 왼쪽에서 글자를 가져옵니다. 개수를 빼면 한 글자입니다.');
    case 'RIGHT': return tr('글의 오른쪽에서 글자를 가져옵니다. 개수를 빼면 한 글자입니다.');
    case 'MID': return tr('글의 가운데에서 시작 위치(1부터)와 개수로 가져옵니다.');
    case 'LEN': return tr('글자 수를 셉니다.');
    case 'UPPER': return tr('영문을 대문자로 바꿉니다.');
    case 'LOWER': return tr('영문을 소문자로 바꿉니다.');
    case 'TRIM': return tr('앞뒤 공백을 지우고 가운데 공백은 하나로 줄입니다.');
    case 'SUBSTITUTE': return tr('글에서 찾는 글을 다른 글로 바꿉니다. 네 번째 값은 몇 번째만 바꿀지입니다.');
    case 'REGEXREPLACE': return tr('정규식으로 찾아 바꿉니다. 바꿀 글의 $1 은 첫 번째 묶음입니다.');
    case 'SPLITPART': return tr('구분자로 나눈 n번째 조각입니다. 1부터 세고, 음수는 뒤에서부터 셉니다.');
    case 'IF': return tr('조건이 참이면 앞의 값을, 아니면 뒤의 값을 씁니다. 뒤의 값을 빼면 빈 값입니다.');
    case 'IFS': return tr('조건과 값을 짝지어 적고 처음으로 참인 조건의 값을 씁니다.');
    case 'AND': return tr('모든 조건이 참이면 TRUE 입니다.');
    case 'OR': return tr('조건 하나라도 참이면 TRUE 입니다.');
    case 'NOT': return tr('참과 거짓을 뒤집습니다.');
    case 'ISBLANK': return tr('값이 비었거나 공백뿐이면 TRUE 입니다.');
    case 'IFBLANK': return tr('값이 비었으면 대신할 값을 씁니다.');
    case 'VALUE': return tr('글을 숫자로 바꿉니다. 1,234 와 12% 도 읽습니다.');
    case 'ROUND': return tr('숫자를 반올림합니다. 자릿수가 음수이면 10, 100 단위입니다.');
    case 'INT': return tr('소수점 아래를 버려 정수로 만듭니다. 음수는 더 작은 쪽입니다.');
    case 'ABS': return tr('절댓값입니다.');
    case 'TEXT': return tr('숫자나 날짜를 서식에 맞춰 글로 바꿉니다.');
    case 'YEAR': return tr('날짜의 연도입니다.');
    case 'MONTH': return tr('날짜의 월입니다.');
    case 'DAY': return tr('날짜의 일입니다.');
    default: return '';
  }
}

/** The name `{0} n` that no other computed field has. */
export function newName(names:string[]):string {
  for(let n=1;;n++){const name=tr('계산 열 {0}',{0:n});if(!names.includes(name))return name;}
}

/** The reference to a field in an expression: its name in braces; with @ or # in front when another field has the same name. */
export function fieldRef(field:{name:string;kind:string},all:{name:string;kind:string}[]):string {
  const same=all.filter(f=>f.name===field.name).length>1;
  return `{${same&&field.kind==='image'?'@':same&&field.kind==='qrcode'?'#':''}${field.name}}`;
}

/** The kind a computed field's name makes: @ an image field, # a QR code field. */
export const kindOfName=(name:string):'image'|'qrcode'|'text'=>name.startsWith('@')?'image':name.startsWith('#')?'qrcode':'text';
export const baseName=(name:string)=>name.replace(/^[@#]/,'').trim();
