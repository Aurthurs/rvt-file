module.exports = function (plop) {
  plop.setGenerator('component', {
    description: '生成.vue组件',
    prompts: [
      { type: 'input', name: 'name', message: '组件名称' },
      { type: 'input', name: 'dir', message: '存放目录', default: 'src/components' },
    ],
    actions: [
      {
        type: 'add',
        path: '{{dir}}/{{pascalCase name}}.vue',
        templateFile: 'plop-templates/component.hbs',
      },
    ],
  });
};
