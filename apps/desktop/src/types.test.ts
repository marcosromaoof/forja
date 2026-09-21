import {describe,it,expect} from 'vitest';
import {timeline,type Event} from './types';
describe('event replay',()=>{it('replaces deltas with final content without duplicating a message',()=>{
 const make=(type:string,payload:any,i:number)=>({event_id:String(i),run_id:'r',session_id:'s',sequence:i,timestamp:'',type,payload}) as Event;
 const result=timeline([make('run.started',{goal:'Corrigir'},1),make('message.started',{},2),make('message.delta',{text:'Olá'},3),make('message.completed',{text:'Olá mundo'},4),make('tool.completed',{name:'fs.read_text',output:{success:true}},5)]);
 expect(result).toHaveLength(3);expect(result[1].text).toBe('Olá mundo');expect(result[2].kind).toBe('tool');
});});
